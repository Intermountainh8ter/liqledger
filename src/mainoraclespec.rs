//! `mainoraclespec`: share of Aave V3 liquidation transactions with no Chainlink AnswerUpdated log in an earlier
//! transaction of the same block or in the 2 preceding blocks. Parameters are fixed in preregistration_additions.md.
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

const MIN_LIQS: u64 = 100;

fn summarize(update_txs: &BTreeSet<(u64, u64)>, liq_txs: &BTreeSet<(u64, u64)>) -> serde_json::Value {
    let n = liq_txs.len() as u64;
    let mut earlier = 0u64;
    let mut prior_blocks = 0u64;
    let mut spec = 0u64;
    let mut same_tx_only = 0u64;
    for l in liq_txs {
        let e = update_txs.iter().any(|u| u.0 == l.0 && u.1 < l.1);
        let p = update_txs.iter().any(|u| u.0 < l.0 && u.0 + 2 >= l.0);
        if e {
            earlier += 1;
        }
        if !e && p {
            prior_blocks += 1;
        }
        if !e && !p {
            spec += 1;
            if update_txs.contains(l) {
                same_tx_only += 1;
            }
        }
    }
    let abstain = n < MIN_LIQS;
    let ci = wilson(spec, n);
    serde_json::json!({
        "liquidation_txs_counted": n,
        "earlier_tx_same_block_update": earlier,
        "prior_two_blocks_update_only": prior_blocks,
        "speculative": spec,
        "speculative_of_which_same_tx_update": same_tx_only,
        "speculative_share": if abstain { serde_json::Value::Null } else { serde_json::json!(spec as f64 / n as f64) },
        "wilson_low": if abstain { None } else { ci.map(|c| c.0) },
        "wilson_high": if abstain { None } else { ci.map(|c| c.1) },
        "abstain": abstain,
    })
}

pub async fn mainoraclespec(network: String, start: u64, blocks: u64, out: PathBuf) -> anyhow::Result<()> {
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
    let need: BTreeSet<u64> = liq_blocks.iter().flat_map(|b| [b.saturating_sub(2), b.saturating_sub(1), *b]).collect();
    for b in &need {
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
            "command": format!("mainoraclespec --chainstack-network {network} --start {start} --blocks {blocks} --out {}", out.display()),
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
        let l: BTreeSet<_> = [(1u64, 5u64)].into();
        let s = summarize(&u, &l);
        assert_eq!(s["abstain"], true);
        assert!(s["speculative_share"].is_null());
    }

    #[test]
    fn classifies_four_cases() {
        let u: BTreeSet<_> = [(10u64, 3u64), (20, 4), (30, 7)].into();
        let mut l: BTreeSet<(u64, u64)> = BTreeSet::new();
        l.insert((10, 9)); // earlier tx same block
        l.insert((12, 1)); // update 2 blocks before
        l.insert((13, 1)); // 3 blocks before: speculative
        l.insert((20, 4)); // same tx only: speculative
        l.insert((30, 2)); // update later in block: speculative
        for i in 0..100u64 {
            l.insert((1000 + i, 0)); // far from any update
        }
        let s = summarize(&u, &l);
        assert_eq!(s["earlier_tx_same_block_update"], 1);
        assert_eq!(s["prior_two_blocks_update_only"], 1);
        assert_eq!(s["speculative"], 103);
        assert_eq!(s["speculative_of_which_same_tx_update"], 1);
        assert_eq!(s["abstain"], false);
    }
}
