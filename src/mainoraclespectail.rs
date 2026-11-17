//! `mainoraclespectail`: the tail-minus-rest speculative share of single-log Aave V3 USDC/USDT-debt liquidations
//! with debtToCover at or above 20000 units versus below. Parameters are fixed in preregistration_additions.md.
//! Reads only; the endpoint is never printed.

use crate::defcheck::chrono_now;
use crate::mainoracleimmediate::{chainstack_endpoint, liq_logs, logs_retry, pos};
use crate::mainoracleimmediatesize::stable_debt;
use crate::mainoraclespeccb::is_speculative;
use crate::AnswerUpdated;
use alloy::providers::ProviderBuilder;
use alloy::rpc::types::Filter;
use alloy::sol_types::SolEvent;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

const MIN_TOTAL: usize = 100;
const MIN_BUCKET: usize = 20;
const TAIL_UNITS: u128 = 20_000;

fn share(flags: &[bool]) -> Option<f64> {
    if flags.is_empty() { None } else { Some(flags.iter().filter(|f| **f).count() as f64 / flags.len() as f64) }
}

/// `rows` are (is_speculative, raw debtToCover); the cutoff is in 6-decimal units.
fn summarize(rows: &[(bool, u128)]) -> serde_json::Value {
    let raw = TAIL_UNITS * 1_000_000;
    let rest: Vec<bool> = rows.iter().filter(|r| r.1 < raw).map(|r| r.0).collect();
    let tail: Vec<bool> = rows.iter().filter(|r| r.1 >= raw).map(|r| r.0).collect();
    let abstain = rows.len() < MIN_TOTAL || tail.len() < MIN_BUCKET;
    let (s, t) = (share(&rest), share(&tail));
    let diff = match (s, t) { (Some(s), Some(t)) if !abstain => Some(t - s), _ => None };
    serde_json::json!({
        "threshold_units": TAIL_UNITS as u64, "rest_txs": rest.len(), "tail_txs": tail.len(),
        "rest_spec_share": s, "tail_spec_share": t,
        "tail_minus_rest_spec_share": diff, "abstain": abstain, "txs": rows.len(),
    })
}

pub async fn mainoraclespectail(network: String, start: u64, blocks: u64, out: PathBuf) -> anyhow::Result<()> {
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
    let all = pos(&logs);
    let mut cnt: BTreeMap<(u64, u64), u64> = BTreeMap::new();
    for l in &all {
        *cnt.entry(*l).or_default() += 1;
    }
    let mut stable: Vec<(u64, u64, u128)> =
        stable_debt(&logs).into_iter().filter(|s| cnt.get(&(s.0, s.1)) == Some(&1)).collect();
    stable.sort();
    let need: BTreeSet<u64> = stable.iter().flat_map(|r| [r.0.saturating_sub(2), r.0.saturating_sub(1), r.0]).collect();
    let mut update_txs = BTreeSet::new();
    for b in &need {
        let f = Filter::new().event_signature(AnswerUpdated::SIGNATURE_HASH).from_block(*b).to_block(*b);
        update_txs.extend(pos(&logs_retry(&p, &f).await?));
    }
    let rows: Vec<(bool, u128)> = stable.iter().map(|r| (is_speculative(&update_txs, (r.0, r.1)), r.2)).collect();
    let mut range = summarize(&rows);
    let o = range.as_object_mut().unwrap();
    o.insert("first_block".into(), start.into());
    o.insert("last_block".into(), last.into());
    o.insert("blocks".into(), blocks.into());
    o.insert("liquidation_logs".into(), logs.len().into());
    let v = serde_json::json!({
        "ranges": [range],
        "provenance": {
            "started_at": started_at,
            "seeds": [7],
            "command": format!("mainoraclespectail --chainstack-network {network} --start {start} --blocks {blocks} --out {}", out.display()),
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
    fn abstains_when_tail_is_small() {
        let mut rows: Vec<(bool, u128)> = vec![(true, 100_000_000); 90];
        rows.extend(vec![(false, 30_000_000_000); 10]);
        let s = summarize(&rows);
        assert_eq!(s["abstain"], true);
        assert!(s["tail_minus_rest_spec_share"].is_null());
    }

    #[test]
    fn tail_below_rest_is_negative() {
        let mut rows: Vec<(bool, u128)> = vec![(true, 100_000_000); 80];
        rows.extend(vec![(false, 30_000_000_000); 30]);
        let s = summarize(&rows);
        assert_eq!(s["abstain"], false);
        assert_eq!(s["tail_minus_rest_spec_share"], -1.0);
    }
}
