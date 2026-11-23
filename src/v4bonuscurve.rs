//! `v4bonuscurve`: for each Aave V4 liquidation transaction of a pinned range, compare the landed bonus (collateral value removed over debt value
//! restored, at the block before landing) with the bonus the Spoke reports for the borrower's health factor, and count transactions outside a tolerance.
//! Parameters are fixed in preregistration_additions.md. Reads only; the endpoint is never printed.

use crate::defcheck::chrono_now;
use crate::mainoracleimmediate::{chainstack_endpoint, logs_retry};
use crate::v4census::{fetch_source, parse_spokes, LiquidationCall};
use crate::v4dustrule::{debt_value, IV4Oracle, IV4Spoke};
use alloy::eips::BlockId;
use alloy::primitives::{Address, B256, U256};
use alloy::providers::ProviderBuilder;
use alloy::rpc::types::Filter;
use alloy::sol;
use alloy::sol_types::SolEvent;
use std::collections::BTreeMap;
use std::path::PathBuf;

sol! {
    struct UserAccountData { uint256 riskPremium; uint256 avgCollateralFactor; uint256 healthFactor; uint256 totalCollateralValue; uint256 totalDebtValueRay; uint256 activeCollateralCount; uint256 borrowedCount; }
    #[sol(rpc)]
    interface IV4Bonus {
        function getUserAccountData(address user) external view returns (UserAccountData memory);
        function getLiquidationBonus(uint256 reserveId, address user, uint256 healthFactor) external view returns (uint256);
    }
}

/// Landed bonus in basis points over par: collateral value over debt value minus one; None when the debt value is zero.
pub fn landed_bonus_bps(coll_value: U256, debt_value: U256) -> Option<i128> {
    if debt_value.is_zero() {
        return None;
    }
    let ratio = coll_value.checked_mul(U256::from(10_000u32))? / debt_value;
    let r: u128 = ratio.try_into().ok()?;
    Some(r as i128 - 10_000)
}

/// Documented bonus in basis points over par from a Spoke bonus value in basis points where 10000 is par; None when below par or above 3x par.
pub fn documented_bonus_bps(raw: U256) -> Option<i128> {
    let r: u128 = raw.try_into().ok()?;
    if !(10_000..=30_000).contains(&r) {
        return None;
    }
    Some(r as i128 - 10_000)
}

pub fn outside(landed: i128, documented: i128, tol_bps: i128) -> bool {
    (landed - documented).abs() > tol_bps
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Reason {
    HealthFactorUnreadable,
    BonusUnreadable,
    BonusOutOfRange,
    PriceUnreadable,
    ZeroDebtValue,
}

impl Reason {
    fn name(&self) -> &'static str {
        match self {
            Reason::HealthFactorUnreadable => "health factor unreadable",
            Reason::BonusUnreadable => "bonus unreadable",
            Reason::BonusOutOfRange => "bonus out of range",
            Reason::PriceUnreadable => "price or reserve unreadable",
            Reason::ZeroDebtValue => "zero debt value",
        }
    }
}

type Ev = (u64, Address, Address, U256, U256, U256, U256);

async fn read_val<P: alloy::providers::Provider>(p: &P, spoke: Address, rid: U256, amount: U256, b: u64) -> Option<U256> {
    let t = std::time::Duration::from_secs(30);
    let blk = BlockId::number(b);
    let sp = IV4Spoke::new(spoke, p);
    let oracle = tokio::time::timeout(t, sp.ORACLE().block(blk).call()).await.ok()?.ok()?;
    let res = tokio::time::timeout(t, sp.getReserve(rid).block(blk).call()).await.ok()?.ok()?;
    let o = IV4Oracle::new(oracle, p);
    let price = tokio::time::timeout(t, o.getReservePrice(rid).block(blk).call()).await.ok()?.ok()?;
    let od = tokio::time::timeout(t, o.DECIMALS().block(blk).call()).await.ok()?.ok().unwrap_or(8);
    debt_value(amount, res.decimals, price, od)
}

async fn check_event<P: alloy::providers::Provider>(p: &P, ev: &Ev, tol: i128) -> Result<(i128, i128, bool), Reason> {
    let (b, spoke, user, crid, drid, debt_amt, coll_amt) = *ev;
    let t = std::time::Duration::from_secs(30);
    let blk = BlockId::number(b - 1);
    let sp = IV4Bonus::new(spoke, p);
    let hf = tokio::time::timeout(t, sp.getUserAccountData(user).block(blk).call())
        .await
        .ok()
        .and_then(|r| r.ok())
        .ok_or(Reason::HealthFactorUnreadable)?
        .healthFactor;
    let raw = tokio::time::timeout(t, sp.getLiquidationBonus(drid, user, hf).block(blk).call())
        .await
        .ok()
        .and_then(|r| r.ok())
        .ok_or(Reason::BonusUnreadable)?;
    let doc = documented_bonus_bps(raw).ok_or(Reason::BonusOutOfRange)?;
    let cv = read_val(p, spoke, crid, coll_amt, b - 1).await.ok_or(Reason::PriceUnreadable)?;
    let dv = read_val(p, spoke, drid, debt_amt, b - 1).await.ok_or(Reason::PriceUnreadable)?;
    let landed = landed_bonus_bps(cv, dv).ok_or(Reason::ZeroDebtValue)?;
    Ok((landed, doc, outside(landed, doc, tol)))
}

pub async fn v4bonuscurve(network: String, start: u64, blocks: u64, commit: String, tolerance_bps: u64, seed: u64, out: PathBuf) -> anyhow::Result<()> {
    let started = std::time::Instant::now();
    let started_at = chrono_now();
    anyhow::ensure!(blocks >= 1 && start >= 2, "blocks must be at least 1 and start at least 2");
    let last = start + blocks - 1;
    let found = parse_spokes(&fetch_source(&commit)?);
    let spokes: Vec<Address> = found.iter().filter_map(|(_, a)| a.parse().ok()).collect();
    anyhow::ensure!(!spokes.is_empty(), "no Spokes found at the pinned commit");
    let url = chainstack_endpoint(&network)?;
    let p = ProviderBuilder::new().connect_http(url.parse().map_err(|_| anyhow::anyhow!("bad {network} endpoint"))?);
    let mut txs: BTreeMap<B256, Vec<Ev>> = BTreeMap::new();
    let mut a = start;
    while a <= last {
        let z = (a + 9_999).min(last);
        let f = Filter::new().address(spokes.clone()).event_signature(LiquidationCall::SIGNATURE_HASH).from_block(a).to_block(z);
        for l in logs_retry(&p, &f).await? {
            let (Some(h), Some(b)) = (l.transaction_hash, l.block_number) else {
                anyhow::bail!("ethereum eth_getLogs returned a log without position fields");
            };
            let ev = LiquidationCall::decode_log(&l.inner).map_err(|_| anyhow::anyhow!("ethereum eth_getLogs returned an undecodable LiquidationCall"))?;
            let d = &ev.data;
            txs.entry(h).or_default().push((b, l.address(), d.user, d.collateralReserveId, d.debtReserveId, d.debtAmountRestored, d.collateralAmountRemoved));
        }
        a = z + 1;
    }
    let tol = tolerance_bps as i128;
    let (mut priced, mut abstain, mut outside_n) = (0u64, 0u64, 0u64);
    let mut reasons: BTreeMap<&'static str, u64> = BTreeMap::new();
    let mut max_abs = 0i128;
    for (_h, evs) in txs.iter() {
        let mut tx_out = false;
        let mut fail: Option<Reason> = None;
        for ev in evs {
            match check_event(&p, ev, tol).await {
                Ok((l, d, o)) => {
                    max_abs = max_abs.max((l - d).abs());
                    tx_out |= o;
                }
                Err(r) => {
                    fail.get_or_insert(r);
                }
            }
        }
        match fail {
            Some(r) => {
                abstain += 1;
                *reasons.entry(r.name()).or_default() += 1;
            }
            None => {
                priced += 1;
                if tx_out {
                    outside_n += 1;
                }
            }
        }
    }
    let key = format!("bonus_outside_{tolerance_bps}bps_count");
    let mut v = serde_json::json!({
        "priced_tx_count": priced,
        "abstain_tx_count": abstain,
        "unpriced_by_reason": reasons,
        "max_abs_bonus_diff_bps_priced": max_abs,
        "tolerance_bps": tolerance_bps,
        "v4_liquidation_tx_count": txs.len(),
        "spoke_count": spokes.len(),
        "first_block": start, "last_block": last, "blocks": blocks,
        "address_book_commit": commit,
        "provenance": {
            "started_at": started_at,
            "seeds": [seed],
            "command": format!("v4bonuscurve --chainstack-network {network} --start {start} --blocks {blocks} --address-book-commit {commit} --tolerance-bps {tolerance_bps} --seed {seed} --out {}", out.display()),
            "hardware": format!("{} on {} logical cpus", std::env::consts::ARCH, std::thread::available_parallelism().map(|x| x.get()).unwrap_or(1)),
            "wall_s": started.elapsed().as_secs_f64(),
            "data_sources": [
                {"url": format!("chainstack:{network} blocks {start} to {last} (logs, eth_call at block N-1)"), "license": "public chain data via the owner's Chainstack node"},
                {"url": format!("https://raw.githubusercontent.com/bgd-labs/aave-address-book/{commit}/src/AaveV4Ethereum.sol"), "license": "MIT (bgd-labs aave-address-book)"},
                {"url": "https://aave.com/docs/aave-v4/positions/liquidations", "license": "official docs, bonus curve read for the formula only"}
            ],
        }
    });
    v[key] = serde_json::json!(outside_n);
    std::fs::write(&out, serde_json::to_string_pretty(&v)? + "\n")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn landed_bonus_five_percent() {
        assert_eq!(landed_bonus_bps(U256::from(105u32), U256::from(100u32)), Some(500));
        assert_eq!(landed_bonus_bps(U256::from(1u32), U256::ZERO), None);
    }

    #[test]
    fn documented_bonus_range() {
        assert_eq!(documented_bonus_bps(U256::from(10_500u32)), Some(500));
        assert_eq!(documented_bonus_bps(U256::from(9_999u32)), None);
        assert_eq!(documented_bonus_bps(U256::from(30_001u32)), None);
    }

    #[test]
    fn tolerance_edges() {
        assert!(!outside(500, 400, 100));
        assert!(outside(501, 400, 100));
        assert!(outside(300, 400, 99));
    }
}
