//! `mainoraclespecthresh`: the large-minus-small speculative share of single-log Aave V3 USDC/USDT-debt liquidations
//! at debtToCover cutoffs of 250, 1000 and 4000 units. Parameters are fixed in preregistration_additions.md.
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
const CUTOFF_UNITS: [u64; 3] = [250, 1000, 4000];

fn share(flags: &[bool]) -> Option<f64> {
    if flags.is_empty() { None } else { Some(flags.iter().filter(|f| **f).count() as f64 / flags.len() as f64) }
}

/// `rows` are (is_speculative, raw debtToCover); cutoffs are in 6-decimal units.
fn summarize(rows: &[(bool, u128)]) -> serde_json::Value {
    let mut cuts = Vec::new();
    let mut any_abstain = false;
    let mut negative = 0u64;
    for units in CUTOFF_UNITS {
        let raw = units as u128 * 1_000_000;
        let small: Vec<bool> = rows.iter().filter(|r| r.1 < raw).map(|r| r.0).collect();
        let large: Vec<bool> = rows.iter().filter(|r| r.1 >= raw).map(|r| r.0).collect();
        let abstain = rows.len() < MIN_TOTAL || small.len() < MIN_BUCKET || large.len() < MIN_BUCKET;
        any_abstain |= abstain;
        let (s, l) = (share(&small), share(&large));
        let diff = match (s, l) { (Some(s), Some(l)) if !abstain => Some(l - s), _ => None };
        if matches!(diff, Some(d) if d < 0.0) {
            negative += 1;
        }
        cuts.push(serde_json::json!({
            "threshold_units": units, "small_txs": small.len(), "large_txs": large.len(),
            "small_spec_share": s, "large_spec_share": l,
            "large_minus_small_spec_share": diff, "abstain": abstain,
        }));
    }
    serde_json::json!({
        "cutoffs": cuts, "abstain": any_abstain, "txs": rows.len(),
        "negative_cutoff_count": if any_abstain { serde_json::Value::Null } else { negative.into() },
    })
}

pub async fn mainoraclespecthresh(network: String, start: u64, blocks: u64, out: PathBuf) -> anyhow::Result<()> {
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
            "command": format!("mainoraclespecthresh --chainstack-network {network} --start {start} --blocks {blocks} --out {}", out.display()),
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
    fn abstains_when_a_cutoff_bucket_is_small() {
        // 100 txs, but only 10 at or above 4000 units
        let mut rows: Vec<(bool, u128)> = vec![(true, 100_000_000); 90];
        rows.extend(vec![(false, 5_000_000_000); 10]);
        let s = summarize(&rows);
        assert_eq!(s["abstain"], true);
        assert!(s["negative_cutoff_count"].is_null());
    }

    #[test]
    fn counts_negative_cutoffs() {
        // small (below every cutoff) speculative, large not: negative at all three
        let mut rows: Vec<(bool, u128)> = vec![(true, 100_000_000); 30];
        rows.extend(vec![(true, 500_000_000); 30]);
        rows.extend(vec![(false, 2_000_000_000); 30]);
        rows.extend(vec![(false, 5_000_000_000); 30]);
        let s = summarize(&rows);
        assert_eq!(s["abstain"], false);
        assert_eq!(s["negative_cutoff_count"], 3);
    }
}
