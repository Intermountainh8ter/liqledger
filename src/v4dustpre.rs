//! `v4dustpre`: repeat the v4dustrule scan and, for each V4 liquidation transaction leaving nonzero debt valued below the dust threshold,
//! read the borrower's debt value at the block before landing and count those whose pre-liquidation debt was at or above the threshold.
//! Parameters are fixed in preregistration_additions.md. Reads only; the endpoint is never printed.

use crate::defcheck::chrono_now;
use crate::mainoracleimmediate::{chainstack_endpoint, logs_retry};
use crate::v4census::{fetch_source, parse_spokes, LiquidationCall};
use crate::v4dustrule::{debt_value, is_dust_violation, IV4Oracle, IV4Spoke, DUST_THRESHOLD_VALUE};
use alloy::eips::BlockId;
use alloy::primitives::{Address, B256, U256};
use alloy::providers::{Provider, ProviderBuilder};
use alloy::rpc::types::Filter;
use alloy::sol_types::SolEvent;
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Classification of one sub-threshold transaction by its pre-liquidation debt values.
#[derive(Debug, PartialEq, Eq)]
pub enum PreClass {
    AboveThreshold,
    BelowThreshold,
}

/// A transaction is above-threshold only if every (nonzero-remainder, sub-threshold) event had pre-debt value at or above the threshold.
pub fn classify_pre(pre_values: &[U256], threshold: u128) -> PreClass {
    if !pre_values.is_empty() && pre_values.iter().all(|v| *v >= U256::from(threshold)) {
        PreClass::AboveThreshold
    } else {
        PreClass::BelowThreshold
    }
}

async fn read_value<P: Provider>(p: &P, spoke: Address, user: Address, rid: U256, b: u64) -> Option<(U256, U256)> {
    let t = std::time::Duration::from_secs(30);
    let blk = BlockId::number(b);
    let sp = IV4Spoke::new(spoke, p);
    let debt = tokio::time::timeout(t, sp.getUserTotalDebt(rid, user).block(blk).call()).await.ok()?.ok()?;
    let oracle = tokio::time::timeout(t, sp.ORACLE().block(blk).call()).await.ok()?.ok()?;
    let res = tokio::time::timeout(t, sp.getReserve(rid).block(blk).call()).await.ok()?.ok()?;
    let o = IV4Oracle::new(oracle, p);
    let price = tokio::time::timeout(t, o.getReservePrice(rid).block(blk).call()).await.ok()?.ok()?;
    let od = tokio::time::timeout(t, o.DECIMALS().block(blk).call()).await.ok()?.ok().unwrap_or(8);
    debt_value(debt, res.decimals, price, od).map(|v| (debt, v))
}

pub async fn v4dustpre(network: String, start: u64, blocks: u64, commit: String, seed: u64, out: PathBuf) -> anyhow::Result<()> {
    let started = std::time::Instant::now();
    let started_at = chrono_now();
    anyhow::ensure!(blocks >= 1 && start >= 1, "blocks must be at least 1 and start at least 1");
    let last = start + blocks - 1;
    let found = parse_spokes(&fetch_source(&commit)?);
    let spokes: Vec<Address> = found.iter().filter_map(|(_, a)| a.parse().ok()).collect();
    anyhow::ensure!(!spokes.is_empty(), "no Spokes found at the pinned commit");
    let url = chainstack_endpoint(&network)?;
    let p = ProviderBuilder::new().connect_http(url.parse().map_err(|_| anyhow::anyhow!("bad {network} endpoint"))?);
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
    let (mut priced, mut abstain, mut sub, mut above_pre, mut below_pre, mut pre_abstain) = (0u64, 0u64, 0u64, 0u64, 0u64, 0u64);
    for (_h, evs) in txs.iter() {
        let mut unreadable = false;
        let mut viol_events: Vec<(u64, Address, Address, U256)> = Vec::new();
        for (b, spoke, user, rid) in evs {
            match read_value(&p, *spoke, *user, *rid, *b).await {
                Some((d, v)) => {
                    if is_dust_violation(d, v, DUST_THRESHOLD_VALUE) {
                        viol_events.push((*b, *spoke, *user, *rid));
                    }
                }
                None => unreadable = true,
            }
        }
        if unreadable {
            abstain += 1;
            continue;
        }
        priced += 1;
        if viol_events.is_empty() {
            continue;
        }
        sub += 1;
        let mut pre_vals = Vec::new();
        let mut pre_ok = true;
        for (b, spoke, user, rid) in &viol_events {
            match read_value(&p, *spoke, *user, *rid, *b - 1).await {
                Some((_, v)) => pre_vals.push(v),
                None => pre_ok = false,
            }
        }
        if !pre_ok {
            pre_abstain += 1;
        } else if classify_pre(&pre_vals, DUST_THRESHOLD_VALUE) == PreClass::AboveThreshold {
            above_pre += 1;
        } else {
            below_pre += 1;
        }
    }
    let v = serde_json::json!({
        "subthreshold_with_abovethreshold_predebt_count": above_pre,
        "subthreshold_with_belowthreshold_predebt_count": below_pre,
        "subthreshold_predebt_unreadable_count": pre_abstain,
        "subthreshold_tx_count": sub,
        "priced_tx_count": priced,
        "abstain_tx_count": abstain,
        "dust_threshold_value_1e26_units": DUST_THRESHOLD_VALUE.to_string(),
        "v4_liquidation_tx_count": txs.len(),
        "spoke_count": spokes.len(),
        "first_block": start, "last_block": last, "blocks": blocks,
        "address_book_commit": commit,
        "provenance": {
            "started_at": started_at,
            "seeds": [seed],
            "command": format!("v4dustpre --chainstack-network {network} --start {start} --blocks {blocks} --address-book-commit {commit} --seed {seed} --out {}", out.display()),
            "hardware": format!("{} on {} logical cpus", std::env::consts::ARCH, std::thread::available_parallelism().map(|x| x.get()).unwrap_or(1)),
            "wall_s": started.elapsed().as_secs_f64(),
            "data_sources": [
                {"url": format!("chainstack:{network} blocks {start} to {last} (logs, eth_call at blocks N-1 and N)"), "license": "public chain data via the owner's Chainstack node"},
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
    fn all_above_is_above() {
        let t = DUST_THRESHOLD_VALUE;
        assert_eq!(classify_pre(&[U256::from(t), U256::from(t + 5)], t), PreClass::AboveThreshold);
    }

    #[test]
    fn any_below_or_empty_is_below() {
        let t = DUST_THRESHOLD_VALUE;
        assert_eq!(classify_pre(&[U256::from(t), U256::from(t - 1)], t), PreClass::BelowThreshold);
        assert_eq!(classify_pre(&[], t), PreClass::BelowThreshold);
    }
}
