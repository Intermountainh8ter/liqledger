//! `mainoracleimmediatesplit`: the `mainoracleimmediate` count split by the number of LiquidationCall logs in the
//! transaction (1 versus 2 or more, a labelled size proxy), with Wilson intervals. Parameters are fixed in
//! preregistration_additions.md. Reads only; the endpoint is never printed.

use crate::defcheck::chrono_now;
use crate::mainoracleimmediate::{chainstack_endpoint, liq_logs, logs_retry, pos};
use crate::AnswerUpdated;
use alloy::providers::ProviderBuilder;
use alloy::rpc::types::Filter;
use alloy::sol_types::SolEvent;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

const MIN_TXS: u64 = 20;

/// Per transaction (block, index): (log count, immediately follows an update), only in blocks holding an update.
pub fn split(liqs: &[(u64, u64)], updates: &[(u64, u64)]) -> Vec<(u64, u64, u64, bool)> {
    let upd: BTreeSet<(u64, u64)> = updates.iter().copied().collect();
    let blocks: BTreeSet<u64> = updates.iter().map(|u| u.0).collect();
    let mut cnt: BTreeMap<(u64, u64), u64> = BTreeMap::new();
    for l in liqs.iter().filter(|l| blocks.contains(&l.0)) {
        *cnt.entry(*l).or_default() += 1;
    }
    cnt.into_iter().map(|((b, i), c)| (b, i, c, i > 0 && upd.contains(&(b, i - 1)))).collect()
}

/// Wilson 95% interval for k of n.
pub fn wilson(k: u64, n: u64) -> Option<(f64, f64)> {
    if n == 0 {
        return None;
    }
    let (k, n, z) = (k as f64, n as f64, 1.96_f64);
    let p = k / n;
    let d = 1.0 + z * z / n;
    let c = (p + z * z / (2.0 * n)) / d;
    let h = z * (p * (1.0 - p) / n + z * z / (4.0 * n * n)).sqrt() / d;
    Some(((c - h).max(0.0), (c + h).min(1.0)))
}

fn bucket(label: &str, rows: &[&(u64, u64, u64, bool)]) -> serde_json::Value {
    let n = rows.len() as u64;
    let hit = rows.iter().filter(|r| r.3).count() as u64;
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

pub async fn mainoracleimmediatesplit(network: String, start: u64, blocks: u64, out: PathBuf) -> anyhow::Result<()> {
    let started = std::time::Instant::now();
    let started_at = chrono_now();
    anyhow::ensure!(blocks >= 1, "blocks must be at least 1");
    let url = chainstack_endpoint(&network)?;
    let p = ProviderBuilder::new().connect_http(url.parse().map_err(|_| anyhow::anyhow!("bad {network} endpoint"))?);
    let last = start + blocks - 1;
    let mut liqs = Vec::new();
    let mut a = start;
    while a <= last {
        let z = (a + 9_999).min(last);
        liqs.extend(pos(&liq_logs(&p, a, z).await?));
        a = z + 1;
    }
    let liq_blocks: BTreeSet<u64> = liqs.iter().map(|l| l.0).collect();
    let mut updates = Vec::new();
    for b in &liq_blocks {
        let f = Filter::new().event_signature(AnswerUpdated::SIGNATURE_HASH).from_block(*b).to_block(*b);
        updates.extend(pos(&logs_retry(&p, &f).await?));
    }
    let m = split(&liqs, &updates);
    let one: Vec<_> = m.iter().filter(|r| r.2 == 1).collect();
    let multi: Vec<_> = m.iter().filter(|r| r.2 >= 2).collect();
    let v = serde_json::json!({
        "ranges": [{
            "first_block": start, "last_block": last, "blocks": blocks,
            "liquidation_logs": liqs.len(), "liquidation_blocks": liq_blocks.len(),
            "buckets": [bucket("one_log", &one), bucket("two_or_more_logs", &multi)],
        }],
        "provenance": {
            "started_at": started_at,
            "seeds": [7],
            "command": format!("mainoracleimmediatesplit --chainstack-network {network} --start {start} --blocks {blocks} --out {}", out.display()),
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
    fn split_counts_logs_per_tx() {
        let r = split(&[(10, 5), (10, 5), (10, 9), (11, 3), (12, 0)], &[(10, 4), (11, 7)]);
        assert_eq!(r, vec![(10, 5, 2, true), (10, 9, 1, false), (11, 3, 1, false)]);
    }

    #[test]
    fn wilson_bounds() {
        assert_eq!(wilson(0, 0), None);
        let (l, h) = wilson(41, 93).unwrap();
        assert!(l < 0.44 && h > 0.44 && l > 0.3 && h < 0.6);
    }
}
