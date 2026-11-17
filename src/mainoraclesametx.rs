//! `mainoraclesametx`: among Aave V3 liquidation transactions in blocks that also hold a Chainlink AnswerUpdated log,
//! the share whose own transaction carries an AnswerUpdated log. Parameters are fixed in preregistration_additions.md.
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

const MIN_LIQS: u64 = 50;

fn summarize(update_txs: &BTreeSet<(u64, u64)>, liq_txs: &BTreeSet<(u64, u64)>) -> serde_json::Value {
    let upd_blocks: BTreeSet<u64> = update_txs.iter().map(|u| u.0).collect();
    let same_block: Vec<&(u64, u64)> = liq_txs.iter().filter(|l| upd_blocks.contains(&l.0)).collect();
    let n = same_block.len() as u64;
    let same_tx = same_block.iter().filter(|l| update_txs.contains(**l)).count() as u64;
    let after = same_block
        .iter()
        .filter(|l| !update_txs.contains(**l) && update_txs.iter().any(|u| u.0 == l.0 && u.1 < l.1))
        .count() as u64;
    let before_only = n - same_tx - after;
    let abstain = n < MIN_LIQS;
    let ci = wilson(same_tx, n);
    serde_json::json!({
        "same_block_liquidation_txs": n,
        "same_tx_as_oracle_update": same_tx,
        "after_update_in_earlier_tx": after,
        "before_all_updates_in_block": before_only,
        "same_tx_as_oracle_update_share": if abstain { serde_json::Value::Null } else { serde_json::json!(same_tx as f64 / n as f64) },
        "wilson_low": if abstain { None } else { ci.map(|c| c.0) },
        "wilson_high": if abstain { None } else { ci.map(|c| c.1) },
        "abstain": abstain,
    })
}

pub async fn mainoraclesametx(network: String, start: u64, blocks: u64, out: PathBuf) -> anyhow::Result<()> {
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
            "command": format!("mainoraclesametx --chainstack-network {network} --start {start} --blocks {blocks} --out {}", out.display()),
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
        let u: BTreeSet<_> = [(1u64, 2u64)].into();
        let l: BTreeSet<_> = [(1u64, 2u64), (1, 5)].into();
        let s = summarize(&u, &l);
        assert_eq!(s["abstain"], true);
        assert!(s["same_tx_as_oracle_update_share"].is_null());
        assert_eq!(s["same_tx_as_oracle_update"], 1);
    }

    #[test]
    fn splits_three_classes_and_ignores_other_blocks() {
        let u: BTreeSet<_> = (0..60).map(|b| (b as u64, 10u64)).collect();
        let mut l: BTreeSet<(u64, u64)> = BTreeSet::new();
        for b in 0..60u64 {
            l.insert((b, 20)); // after update
        }
        for b in 0..10u64 {
            l.insert((b, 10)); // same tx
        }
        l.insert((0, 3)); // before all updates
        l.insert((500, 1)); // block without update: excluded
        let s = summarize(&u, &l);
        assert_eq!(s["same_block_liquidation_txs"], 71);
        assert_eq!(s["same_tx_as_oracle_update"], 10);
        assert_eq!(s["after_update_in_earlier_tx"], 60);
        assert_eq!(s["before_all_updates_in_block"], 1);
        assert_eq!(s["abstain"], false);
    }
}
