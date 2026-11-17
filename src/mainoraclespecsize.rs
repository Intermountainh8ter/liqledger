//! `mainoraclespecsize`: among single-log Aave V3 USDC/USDT-debt liquidation transactions, the speculative share (no
//! AnswerUpdated in an earlier transaction of the same block or the 2 preceding blocks) for debtToCover below versus
//! at least 1000 units. Parameters are fixed in preregistration_additions.md. Reads only; the endpoint is never printed.

use crate::defcheck::chrono_now;
use crate::mainoracleimmediate::{chainstack_endpoint, liq_logs, logs_retry, pos};
use crate::mainoracleimmediatesize::stable_debt;
use crate::mainoraclesizecb::buckets;
use crate::mainoraclespeccb::is_speculative;
use crate::AnswerUpdated;
use alloy::providers::ProviderBuilder;
use alloy::rpc::types::Filter;
use alloy::sol_types::SolEvent;
use std::collections::BTreeSet;
use std::path::PathBuf;

const MIN_TOTAL: usize = 100;
const MIN_BUCKET: usize = 20;

fn share(flags: &[bool]) -> Option<f64> {
    if flags.is_empty() { None } else { Some(flags.iter().filter(|f| **f).count() as f64 / flags.len() as f64) }
}

fn summarize(small: &[bool], large: &[bool]) -> serde_json::Value {
    let abstain = small.len() + large.len() < MIN_TOTAL || small.len() < MIN_BUCKET || large.len() < MIN_BUCKET;
    let (s, l) = (share(small), share(large));
    let diff = match (s, l) { (Some(s), Some(l)) if !abstain => Some(l - s), _ => None };
    serde_json::json!({
        "small_txs": small.len(), "large_txs": large.len(),
        "small_spec_share": s, "large_spec_share": l,
        "large_minus_small_spec_share": diff, "abstain": abstain,
    })
}

pub async fn mainoraclespecsize(network: String, start: u64, blocks: u64, out: PathBuf) -> anyhow::Result<()> {
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
    let rows = buckets(&stable_debt(&logs), &all);
    let need: BTreeSet<u64> = rows.iter().flat_map(|r| [r.0.saturating_sub(2), r.0.saturating_sub(1), r.0]).collect();
    let mut update_txs = BTreeSet::new();
    for b in &need {
        let f = Filter::new().event_signature(AnswerUpdated::SIGNATURE_HASH).from_block(*b).to_block(*b);
        update_txs.extend(pos(&logs_retry(&p, &f).await?));
    }
    let mut flags: [Vec<bool>; 2] = [Vec::new(), Vec::new()];
    for r in &rows {
        flags[r.2].push(is_speculative(&update_txs, (r.0, r.1)));
    }
    let mut range = summarize(&flags[0], &flags[1]);
    let o = range.as_object_mut().unwrap();
    o.insert("first_block".into(), start.into());
    o.insert("last_block".into(), last.into());
    o.insert("blocks".into(), blocks.into());
    o.insert("liquidation_logs".into(), logs.len().into());
    o.insert("threshold_units".into(), 1000.into());
    let v = serde_json::json!({
        "ranges": [range],
        "provenance": {
            "started_at": started_at,
            "seeds": [7],
            "command": format!("mainoraclespecsize --chainstack-network {network} --start {start} --blocks {blocks} --out {}", out.display()),
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
    fn abstains_on_small_bucket() {
        let s = summarize(&vec![true; 90], &vec![false; 10]);
        assert_eq!(s["abstain"], true);
        assert!(s["large_minus_small_spec_share"].is_null());
    }

    #[test]
    fn diff_is_large_minus_small() {
        let mut small = vec![true; 60];
        small.extend(vec![false; 40]);
        let mut large = vec![true; 10];
        large.extend(vec![false; 30]);
        let s = summarize(&small, &large);
        assert_eq!(s["abstain"], false);
        assert!((s["large_minus_small_spec_share"].as_f64().unwrap() - (-0.35)).abs() < 1e-12);
    }
}
