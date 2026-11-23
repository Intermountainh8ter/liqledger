//! `v4dustrule`: for each Aave V4 liquidation transaction of a pinned range, read the borrower's remaining debt in the target reserve at the
//! landing block, price it from the Spoke oracle at that block, and count liquidations that left nonzero debt valued below the dust threshold.
//! Parameters are fixed in preregistration_additions.md. Reads only; the endpoint is never printed.

use crate::defcheck::chrono_now;
use crate::mainoracleimmediate::{chainstack_endpoint, logs_retry};
use crate::v4census::{fetch_source, parse_spokes, LiquidationCall};
use alloy::eips::BlockId;
use alloy::primitives::{Address, B256, U256};
use alloy::providers::ProviderBuilder;
use alloy::rpc::types::Filter;
use alloy::sol;
use alloy::sol_types::SolEvent;
use std::collections::BTreeMap;
use std::path::PathBuf;

sol! {
    #[sol(rpc)]
    interface IV4Spoke {
        function getUserTotalDebt(uint256 reserveId, address user) external view returns (uint256);
        function ORACLE() external view returns (address);
        function getReserve(uint256 reserveId) external view returns (address underlying, address hub, uint16 assetId, uint8 decimals, uint24 dynamicConfigKey, uint16 collateralRisk, uint8 flags);
    }
    #[sol(rpc)]
    interface IV4Oracle {
        function getReservePrice(uint256 reserveId) external view returns (uint256);
        function DECIMALS() external view returns (uint8);
    }
}

/// Documented V4 dust threshold in value units (1 USD = 1e26): 1000 USD.
pub const DUST_THRESHOLD_VALUE: u128 = 1000 * 10u128.pow(26 - 8) * 10u128.pow(8);

/// Debt value in 1e26 units from raw debt, asset decimals, oracle price and oracle decimals; None on overflow or oracle decimals above 26.
pub fn debt_value(debt: U256, asset_dec: u8, price: U256, oracle_dec: u8) -> Option<U256> {
    if oracle_dec > 26 || asset_dec > 77 {
        return None;
    }
    let num = debt.checked_mul(price)?.checked_mul(U256::from(10u8).pow(U256::from(26 - oracle_dec)))?;
    Some(num / U256::from(10u8).pow(U256::from(asset_dec)))
}

/// True when the remainder is nonzero and its value is under the threshold.
pub fn is_dust_violation(debt: U256, value: U256, threshold: u128) -> bool {
    !debt.is_zero() && value < U256::from(threshold)
}

pub async fn v4dustrule(network: String, start: u64, blocks: u64, commit: String, seed: u64, out: PathBuf) -> anyhow::Result<()> {
    let started = std::time::Instant::now();
    let started_at = chrono_now();
    anyhow::ensure!(blocks >= 1, "blocks must be at least 1");
    let last = start + blocks - 1;
    let found = parse_spokes(&fetch_source(&commit)?);
    let spokes: Vec<Address> = found.iter().filter_map(|(_, a)| a.parse().ok()).collect();
    anyhow::ensure!(!spokes.is_empty(), "no Spokes found at the pinned commit");
    let url = chainstack_endpoint(&network)?;
    let p = ProviderBuilder::new().connect_http(url.parse().map_err(|_| anyhow::anyhow!("bad {network} endpoint"))?);
    // tx -> list of (block, spoke, user, debt reserve id)
    let mut txs: BTreeMap<B256, Vec<(u64, Address, Address, U256)>> = BTreeMap::new();
    let mut a = start;
    while a <= last {
        let z = (a + 9_999).min(last);
        let f = Filter::new().address(spokes.clone()).event_signature(LiquidationCall::SIGNATURE_HASH).from_block(a).to_block(z);
        for l in logs_retry(&p, &f).await? {
            let (Some(h), Some(b)) = (l.transaction_hash, l.block_number) else {
                anyhow::bail!("ethereum eth_getLogs returned a log without position fields");
            };
            let ev = LiquidationCall::decode_log(&l.inner).map_err(|_| anyhow::anyhow!("ethereum eth_getLogs returned an undecodable LiquidationCall"))?;
            txs.entry(h).or_default().push((b, l.address(), ev.data.user, ev.data.debtReserveId));
        }
        a = z + 1;
    }
    let t = std::time::Duration::from_secs(30);
    let (mut priced, mut abstain, mut zero_rem, mut dust, mut above) = (0u64, 0u64, 0u64, 0u64, 0u64);
    let mut violations = 0u64;
    for (_h, evs) in txs.iter() {
        let mut tx_unreadable = false;
        let mut tx_viol = false;
        let mut tx_zero_only = true;
        for (b, spoke, user, rid) in evs {
            let blk = BlockId::number(*b);
            let sp = IV4Spoke::new(*spoke, &p);
            let r = async {
                let debt = tokio::time::timeout(t, sp.getUserTotalDebt(*rid, *user).block(blk).call()).await.ok()?.ok()?;
                let oracle = tokio::time::timeout(t, sp.ORACLE().block(blk).call()).await.ok()?.ok()?;
                let res = tokio::time::timeout(t, sp.getReserve(*rid).block(blk).call()).await.ok()?.ok()?;
                let o = IV4Oracle::new(oracle, &p);
                let price = tokio::time::timeout(t, o.getReservePrice(*rid).block(blk).call()).await.ok()?.ok()?;
                let od = tokio::time::timeout(t, o.DECIMALS().block(blk).call()).await.ok()?.ok().unwrap_or(8);
                Some((debt, res.decimals, price, od))
            }
            .await;
            match r.and_then(|(d, ad, pr, od)| debt_value(d, ad, pr, od).map(|v| (d, v))) {
                Some((d, v)) => {
                    if d.is_zero() {
                        continue;
                    }
                    tx_zero_only = false;
                    if is_dust_violation(d, v, DUST_THRESHOLD_VALUE) {
                        tx_viol = true;
                    }
                }
                None => tx_unreadable = true,
            }
        }
        if tx_unreadable {
            abstain += 1;
        } else {
            priced += 1;
            if tx_zero_only {
                zero_rem += 1;
            } else if tx_viol {
                dust += 1;
                violations += 1;
            } else {
                above += 1;
            }
        }
    }
    let v = serde_json::json!({
        "dust_rule_violation_count": violations,
        "priced_tx_count": priced,
        "abstain_tx_count": abstain,
        "zero_remaining_tx_count": zero_rem,
        "nonzero_above_threshold_tx_count": above,
        "nonzero_below_threshold_tx_count": dust,
        "dust_threshold_value_1e26_units": DUST_THRESHOLD_VALUE.to_string(),
        "threshold_source": "documented 1000 USD (1e26 value units per USD), https://aave.com/docs/aave-v4/positions/liquidations",
        "v4_liquidation_tx_count": txs.len(),
        "spoke_count": spokes.len(),
        "first_block": start, "last_block": last, "blocks": blocks,
        "address_book_commit": commit,
        "provenance": {
            "started_at": started_at,
            "seeds": [seed],
            "command": format!("v4dustrule --chainstack-network {network} --start {start} --blocks {blocks} --address-book-commit {commit} --seed {seed} --out {}", out.display()),
            "hardware": format!("{} on {} logical cpus", std::env::consts::ARCH, std::thread::available_parallelism().map(|x| x.get()).unwrap_or(1)),
            "wall_s": started.elapsed().as_secs_f64(),
            "data_sources": [
                {"url": format!("chainstack:{network} blocks {start} to {last} (logs, eth_call at landing block)"), "license": "public chain data via the owner's Chainstack node"},
                {"url": format!("https://raw.githubusercontent.com/bgd-labs/aave-address-book/{commit}/src/AaveV4Ethereum.sol"), "license": "MIT (bgd-labs aave-address-book)"},
                {"url": "https://aave.com/docs/aave-v4/positions/liquidations", "license": "official docs, dust rule read for the threshold definition only"}
            ],
        }
    });
    std::fs::write(&out, serde_json::to_string_pretty(&v)? + "\n")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn threshold_is_1000_usd() {
        assert_eq!(DUST_THRESHOLD_VALUE, 1000u128 * 10u128.pow(26));
    }

    #[test]
    fn value_scales_by_decimals() {
        // 500 tokens of 6 decimals at price 1.00 (8 decimals) = 500 USD = 500e26
        let v = debt_value(U256::from(500_000_000u64), 6, U256::from(100_000_000u64), 8).unwrap();
        assert_eq!(v, U256::from(500u128 * 10u128.pow(26)));
        assert!(debt_value(U256::from(1u8), 6, U256::from(1u8), 30).is_none());
    }

    #[test]
    fn dust_needs_nonzero_and_below() {
        let thr = DUST_THRESHOLD_VALUE;
        assert!(!is_dust_violation(U256::ZERO, U256::ZERO, thr));
        assert!(is_dust_violation(U256::from(1u8), U256::from(thr - 1), thr));
        assert!(!is_dust_violation(U256::from(1u8), U256::from(thr), thr));
    }
}
