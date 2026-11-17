//! `mainoraclespecsweep`: the largest tail speculative share of single-log Aave V3 USDC/USDT-debt liquidations
//! over debtToCover cutoffs of 10000, 20000 and 40000 units. Parameters are fixed in preregistration_additions.md.
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
const CUTOFFS: [u128; 3] = [10_000, 20_000, 40_000];

/// `rows` are (is_speculative, raw debtToCover, 6-decimal units).
fn summarize(rows: &[(bool, u128)]) -> serde_json::Value {
    let mut cuts = Vec::new();
    let mut max: Option<f64> = None;
    for c in CUTOFFS {
        let raw = c * 1_000_000;
        let tail: Vec<bool> = rows.iter().filter(|r| r.1 >= raw).map(|r| r.0).collect();
        let abstain = rows.len() < MIN_TOTAL || tail.len() < MIN_BUCKET;
        let t = if tail.is_empty() { None } else { Some(tail.iter().filter(|f| **f).count() as f64 / tail.len() as f64) };
        if let (Some(t), false) = (t, abstain) {
            max = Some(max.map_or(t, |m: f64| m.max(t)));
        }
        cuts.push(serde_json::json!({"threshold_units": c as u64, "tail_txs": tail.len(), "tail_spec_share": t, "abstain": abstain}));
    }
    serde_json::json!({"cutoffs": cuts, "max_tail_spec_share": max, "abstain": max.is_none(), "txs": rows.len()})
}

pub async fn mainoraclespecsweep(network: String, start: u64, blocks: u64, out: PathBuf) -> anyhow::Result<()> {
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
            "command": format!("mainoraclespecsweep --chainstack-network {network} --start {start} --blocks {blocks} --out {}", out.display()),
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
    fn abstains_when_every_tail_is_small() {
        let mut rows: Vec<(bool, u128)> = vec![(true, 100_000_000); 90];
        rows.extend(vec![(false, 50_000_000_000); 10]);
        let s = summarize(&rows);
        assert_eq!(s["abstain"], true);
        assert!(s["max_tail_spec_share"].is_null());
    }

    #[test]
    fn max_skips_abstaining_cutoffs() {
        let mut rows: Vec<(bool, u128)> = vec![(true, 100_000_000); 80];
        rows.extend(vec![(false, 15_000_000_000); 30]);
        rows.extend(vec![(true, 50_000_000_000); 5]);
        let s = summarize(&rows);
        assert_eq!(s["max_tail_spec_share"], 5.0 / 35.0);
        assert_eq!(s["cutoffs"][2]["abstain"], true);
    }
}
