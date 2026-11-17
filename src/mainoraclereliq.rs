//! `mainoraclereliq`: share of Aave V3 LiquidationCall logs whose borrower already had a LiquidationCall log in the
//! previous 100 blocks (strictly earlier blocks; history before the range start is not read). Parameters are fixed in
//! preregistration_additions.md. Reads only (getLogs); the endpoint is never printed.

use crate::defcheck::chrono_now;
use crate::mainoracleimmediate::{chainstack_endpoint, liq_logs};
use crate::mainoracleimmediatesplit::wilson;
use alloy::providers::ProviderBuilder;
use std::collections::BTreeMap;
use std::path::PathBuf;

const MIN_LOGS: u64 = 100;
const LOOKBACK: u64 = 100;

/// (block, borrower) pairs in block order.
fn summarize(events: &[(u64, [u8; 32])]) -> serde_json::Value {
    let mut last: BTreeMap<[u8; 32], u64> = BTreeMap::new();
    let mut ev: Vec<&(u64, [u8; 32])> = events.iter().collect();
    ev.sort_by_key(|e| e.0);
    let mut re = 0u64;
    let mut i = 0;
    while i < ev.len() {
        let b = ev[i].0;
        let mut j = i;
        while j < ev.len() && ev[j].0 == b {
            j += 1;
        }
        for e in &ev[i..j] {
            if last.get(&e.1).is_some_and(|p| *p < b && b - *p <= LOOKBACK) {
                re += 1;
            }
        }
        for e in &ev[i..j] {
            last.insert(e.1, b);
        }
        i = j;
    }
    let n = ev.len() as u64;
    let abstain = n < MIN_LOGS;
    let ci = wilson(re, n);
    serde_json::json!({
        "liquidation_logs_scored": n,
        "reliq_logs": re,
        "distinct_borrowers": last.len(),
        "reliq_share": if abstain { serde_json::Value::Null } else { serde_json::json!(re as f64 / n as f64) },
        "wilson_low": if abstain { None } else { ci.map(|c| c.0) },
        "wilson_high": if abstain { None } else { ci.map(|c| c.1) },
        "abstain": abstain,
    })
}

pub async fn mainoraclereliq(network: String, start: u64, blocks: u64, out: PathBuf) -> anyhow::Result<()> {
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
    let mut events = Vec::new();
    for l in &logs {
        let t = l.topics();
        if let (Some(b), true) = (l.block_number, t.len() >= 4) {
            events.push((b, t[3].0));
        }
    }
    let mut r = summarize(&events);
    let o = r.as_object_mut().unwrap();
    o.insert("first_block".into(), start.into());
    o.insert("last_block".into(), last.into());
    o.insert("blocks".into(), blocks.into());
    o.insert("liquidation_logs".into(), logs.len().into());
    let v = serde_json::json!({
        "ranges": [r],
        "provenance": {
            "started_at": started_at,
            "seeds": [7],
            "command": format!("mainoraclereliq --chainstack-network {network} --start {start} --blocks {blocks} --out {}", out.display()),
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

    fn k(x: u8) -> [u8; 32] {
        [x; 32]
    }

    #[test]
    fn counts_repeat_within_lookback_only() {
        let mut ev = vec![(1000u64, k(1)), (1050, k(1)), (1200, k(1)), (1200, k(2)), (1200, k(2))];
        for b in 0..100u64 {
            ev.push((5000 + b * 1000, k(9)));
        }
        let s = summarize(&ev);
        assert_eq!(s["reliq_logs"], 1); // 1050 repeats; same-block duplicate and 150-block gap do not
        assert_eq!(s["liquidation_logs_scored"], 105);
        assert_eq!(s["abstain"], false);
    }

    #[test]
    fn abstains_below_minimum() {
        let s = summarize(&[(1, k(1)), (2, k(1))]);
        assert_eq!(s["abstain"], true);
        assert!(s["reliq_share"].is_null());
        assert_eq!(s["reliq_logs"], 1);
    }
}
