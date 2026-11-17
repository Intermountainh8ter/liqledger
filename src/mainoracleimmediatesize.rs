//! `mainoracleimmediatesize`: the `mainoracleimmediate` count split by debtToCover (below or at least 1000 units) for
//! single-log transactions whose debt asset is USDC or USDT (both 6 decimals), with Wilson intervals. Parameters are
//! fixed in preregistration_additions.md. Reads only; the endpoint is never printed.

use crate::defcheck::chrono_now;
use crate::mainoracleimmediate::{chainstack_endpoint, liq_logs, logs_retry, pos};
use crate::mainoracleimmediatesplit::wilson;
use crate::{AnswerUpdated, LiquidationCall};
use alloy::primitives::address;
use alloy::providers::ProviderBuilder;
use alloy::rpc::types::{Filter, Log};
use alloy::sol_types::SolEvent;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

const MIN_TXS: u64 = 20;
/// 1000 units of a 6-decimal stablecoin.
const THRESHOLD_RAW: u128 = 1_000_000_000;

/// (block, tx index, debtToCover raw) for USDC/USDT-debt liquidation logs; others are dropped.
pub fn stable_debt(logs: &[Log]) -> Vec<(u64, u64, u128)> {
    let usdc = address!("A0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48");
    let usdt = address!("dAC17F958D2ee523a2206206994597C13D831ec7");
    logs.iter()
        .filter_map(|l| {
            let ev = LiquidationCall::decode_log(&l.inner).ok()?;
            if ev.debtAsset != usdc && ev.debtAsset != usdt {
                return None;
            }
            let amt: u128 = ev.debtToCover.try_into().unwrap_or(u128::MAX);
            Some((l.block_number?, l.transaction_index?, amt))
        })
        .collect()
}

/// Single-log stable-debt transactions in update blocks: (small or large bucket index, immediately follows an update).
/// `all_counts` holds every LiquidationCall log position (any asset) so multi-log transactions are excluded.
pub fn size_split(
    stable: &[(u64, u64, u128)],
    all_counts: &[(u64, u64)],
    updates: &[(u64, u64)],
) -> Vec<(usize, bool)> {
    let upd: BTreeSet<(u64, u64)> = updates.iter().copied().collect();
    let blocks: BTreeSet<u64> = updates.iter().map(|u| u.0).collect();
    let mut cnt: BTreeMap<(u64, u64), u64> = BTreeMap::new();
    for l in all_counts {
        *cnt.entry(*l).or_default() += 1;
    }
    stable
        .iter()
        .filter(|s| blocks.contains(&s.0) && cnt.get(&(s.0, s.1)) == Some(&1))
        .map(|s| (usize::from(s.2 >= THRESHOLD_RAW), s.1 > 0 && upd.contains(&(s.0, s.1 - 1))))
        .collect()
}

fn bucket(label: &str, rows: &[(usize, bool)], idx: usize) -> serde_json::Value {
    let n = rows.iter().filter(|r| r.0 == idx).count() as u64;
    let hit = rows.iter().filter(|r| r.0 == idx && r.1).count() as u64;
    let ci = wilson(hit, n);
    serde_json::json!({
        "label": label,
        "same_block_liquidation_txs": n,
        "immediate_follow_txs": hit,
        "immediate_follow_share": if n > 0 { serde_json::json!(hit as f64 / n as f64) } else { serde_json::Value::Null },
        "wilson_low": ci.map(|c| c.0),
        "wilson_high": ci.map(|c| c.1),
        "abstain": n < MIN_TXS,
    })
}

pub async fn mainoracleimmediatesize(network: String, start: u64, blocks: u64, out: PathBuf) -> anyhow::Result<()> {
    let started = std::time::Instant::now();
    let started_at = chrono_now();
    anyhow::ensure!(blocks >= 1, "blocks must be at least 1");
    let url = chainstack_endpoint(&network)?;
    let p = ProviderBuilder::new().connect_http(url.parse().map_err(|_| anyhow::anyhow!("bad {network} endpoint"))?);
    let last = start + blocks - 1;
    let mut logs = Vec::new();
    let mut a = start;
    while a <= last {
        let z = (a + 9_999).min(last);
        logs.extend(liq_logs(&p, a, z).await?);
        a = z + 1;
    }
    let liqs = pos(&logs);
    let stable = stable_debt(&logs);
    let liq_blocks: BTreeSet<u64> = liqs.iter().map(|l| l.0).collect();
    let mut updates = Vec::new();
    for b in &liq_blocks {
        let f = Filter::new().event_signature(AnswerUpdated::SIGNATURE_HASH).from_block(*b).to_block(*b);
        updates.extend(pos(&logs_retry(&p, &f).await?));
    }
    let m = size_split(&stable, &liqs, &updates);
    let v = serde_json::json!({
        "ranges": [{
            "first_block": start, "last_block": last, "blocks": blocks,
            "liquidation_logs": liqs.len(), "liquidation_blocks": liq_blocks.len(),
            "threshold_units": 1000, "stable_debt_logs": stable.len(),
            "buckets": [bucket("below_1000_units", &m, 0), bucket("at_least_1000_units", &m, 1)],
        }],
        "provenance": {
            "started_at": started_at,
            "seeds": [7],
            "command": format!("mainoracleimmediatesize --chainstack-network {network} --start {start} --blocks {blocks} --out {}", out.display()),
            "hardware": format!("{} on {} logical cpus", std::env::consts::ARCH, std::thread::available_parallelism().map(|x| x.get()).unwrap_or(1)),
            "wall_s": started.elapsed().as_secs_f64(),
            "data_sources": [
                {"url": format!("chainstack:ethereum blocks {start} to {last} (logs)"),
                 "license": "public chain data via the owner's Chainstack node"}
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
    fn size_split_filters_single_log_and_buckets() {
        let stable = [(10, 5, 2_000_000_000u128), (10, 9, 5u128), (10, 7, 1_000_000_000), (11, 3, 9), (12, 1, 9)];
        let all = [(10, 5), (10, 9), (10, 9), (10, 7), (11, 3), (12, 1)];
        let r = size_split(&stable, &all, &[(10, 4), (10, 6), (11, 7)]);
        assert_eq!(r, vec![(1, true), (1, true), (0, false)]);
    }
}
