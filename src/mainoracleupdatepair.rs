//! `mainoracleupdatepair`: among Chainlink AnswerUpdated transactions in blocks holding an Aave V3 liquidation, the
//! share whose next transaction index holds a liquidation. Parameters are fixed in preregistration_additions.md.
//! Reads only (getLogs); the endpoint is never printed.

use crate::defcheck::chrono_now;
use crate::mainoracleimmediate::{chainstack_endpoint, liq_logs, logs_retry, pos};
use crate::mainoracleimmediatesplit::wilson;
use crate::AnswerUpdated;
use alloy::providers::ProviderBuilder;
use alloy::rpc::types::Filter;
use alloy::sol_types::SolEvent;
use std::collections::BTreeSet;
use std::path::PathBuf;

const MIN_UPDATES: u64 = 20;

fn summarize(update_txs: &BTreeSet<(u64, u64)>, liq_txs: &BTreeSet<(u64, u64)>) -> serde_json::Value {
    let n = update_txs.len() as u64;
    let hit = update_txs.iter().filter(|(b, i)| liq_txs.contains(&(*b, *i + 1))).count() as u64;
    let abstain = n < MIN_UPDATES;
    let ci = wilson(hit, n);
    serde_json::json!({
        "update_txs": n, "update_txs_followed_by_liquidation": hit,
        "update_followed_by_liquidation_share": if abstain { serde_json::Value::Null } else { serde_json::json!(hit as f64 / n as f64) },
        "wilson_low": if abstain { None } else { ci.map(|c| c.0) },
        "wilson_high": if abstain { None } else { ci.map(|c| c.1) },
        "abstain": abstain,
    })
}

pub async fn mainoracleupdatepair(network: String, start: u64, blocks: u64, out: PathBuf) -> anyhow::Result<()> {
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
    let liq_txs: BTreeSet<(u64, u64)> = pos(&logs).into_iter().collect();
    let liq_blocks: BTreeSet<u64> = liq_txs.iter().map(|l| l.0).collect();
    let mut update_txs = BTreeSet::new();
    for b in &liq_blocks {
        let f = Filter::new().event_signature(AnswerUpdated::SIGNATURE_HASH).from_block(*b).to_block(*b);
        update_txs.extend(pos(&logs_retry(&p, &f).await?));
    }
    let mut r = summarize(&update_txs, &liq_txs);
    let o = r.as_object_mut().unwrap();
    o.insert("first_block".into(), start.into());
    o.insert("last_block".into(), last.into());
    o.insert("blocks".into(), blocks.into());
    o.insert("liquidation_logs".into(), logs.len().into());
    o.insert("liquidation_blocks".into(), liq_blocks.len().into());
    o.insert("liquidation_txs".into(), liq_txs.len().into());
    let v = serde_json::json!({
        "ranges": [r],
        "provenance": {
            "started_at": started_at,
            "seeds": [7],
            "command": format!("mainoracleupdatepair --chainstack-network {network} --start {start} --blocks {blocks} --out {}", out.display()),
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
    fn abstains_below_minimum() {
        let u: BTreeSet<_> = (0..5).map(|i| (1u64, i * 3)).collect();
        let l: BTreeSet<_> = (0..5).map(|i| (1u64, i * 3 + 1)).collect();
        let s = summarize(&u, &l);
        assert_eq!(s["abstain"], true);
        assert!(s["update_followed_by_liquidation_share"].is_null());
        assert_eq!(s["update_txs_followed_by_liquidation"], 5);
    }

    #[test]
    fn share_counts_next_index_only() {
        let u: BTreeSet<_> = (0..40).map(|i| (7u64, i * 3)).collect();
        let l: BTreeSet<_> = (0..10).map(|i| (7u64, i * 3 + 1)).chain([(7u64, 2)]).collect();
        let s = summarize(&u, &l);
        assert_eq!(s["update_followed_by_liquidation_share"].as_f64().unwrap(), 0.25);
        assert_eq!(s["abstain"], false);
    }
}
