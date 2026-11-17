//! `mainoracleimmediatecb`: among Aave V3 liquidation transactions directly after a Chainlink AnswerUpdated log, the
//! share with a call of nonzero ETH value to the block header coinbase (trace_transaction), split by zero versus nonzero
//! priority fee, with Wilson intervals. Parameters are fixed in preregistration_additions.md. Reads only; the endpoint
//! is never printed.

use crate::defcheck::chrono_now;
use crate::mainoracleimmediatecb::pays_coinbase;
use crate::mainoracleimmediate::{chainstack_endpoint, liq_logs, logs_retry, pos};
use crate::mainoracleimmediatesplit::wilson;
use crate::AnswerUpdated;
use alloy::providers::{Provider, ProviderBuilder};
use alloy::rpc::types::Filter;
use alloy::sol_types::SolEvent;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;


const MIN_TXS: u64 = 20;

fn arm(label: &str, rows: &[bool]) -> serde_json::Value {
    let n = rows.len() as u64;
    let hit = rows.iter().filter(|r| **r).count() as u64;
    let ci = wilson(hit, n);
    let share = if n > 0 { serde_json::json!(hit as f64 / n as f64) } else { serde_json::Value::Null };
    serde_json::json!({
        "label": label, "txs": n, "coinbase_transfer_txs": hit, "coinbase_transfer_share": share,
        "wilson_low": ci.map(|c| c.0), "wilson_high": ci.map(|c| c.1), "abstain": n < MIN_TXS,
    })
}

async fn raw(p: &impl Provider, method: &'static str, params: serde_json::Value) -> anyhow::Result<serde_json::Value> {
    let mut tries = 0;
    loop {
        match p.raw_request::<_, serde_json::Value>(method.into(), params.clone()).await {
            Ok(v) if !v.is_null() => return Ok(v),
            _ if tries < 5 => {
                tries += 1;
                tokio::time::sleep(std::time::Duration::from_millis(500 * (1 << tries))).await;
            }
            _ => anyhow::bail!("ethereum {method} failed"),
        }
    }
}

pub async fn mainoracleimmediatecbsplit(network: String, start: u64, blocks: u64, out: PathBuf) -> anyhow::Result<()> {
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
    let liq_blocks: BTreeSet<u64> = pos(&logs).iter().map(|l| l.0).collect();
    let mut updates = Vec::new();
    for b in &liq_blocks {
        let f = Filter::new().event_signature(AnswerUpdated::SIGNATURE_HASH).from_block(*b).to_block(*b);
        updates.extend(pos(&logs_retry(&p, &f).await?));
    }
    let upd: BTreeSet<(u64, u64)> = updates.iter().copied().collect();
    let mut txs: BTreeMap<(u64, u64), alloy::primitives::B256> = BTreeMap::new();
    for l in &logs {
        if let (Some(b), Some(i), Some(h)) = (l.block_number, l.transaction_index, l.transaction_hash) {
            txs.insert((b, i), h);
        }
    }
    let mut miners: BTreeMap<u64, String> = BTreeMap::new();
    let (mut imm, mut oth) = (Vec::new(), Vec::new());
    for ((b, i), h) in &txs {
        if !miners.contains_key(b) {
            let hd = raw(&p, "eth_getBlockByNumber", serde_json::json!([format!("0x{b:x}"), false])).await?;
            let m = hd["miner"].as_str().ok_or_else(|| anyhow::anyhow!("no miner"))?.to_string();
            miners.insert(*b, m);
        }
        let tr = raw(&p, "trace_transaction", serde_json::json!([format!("{h:#x}")])).await?;
        let paid = pays_coinbase(&tr, &miners[b]);
        if *i > 0 && upd.contains(&(*b, *i - 1)) { imm.push(paid) } else { oth.push(paid) }
    }
    let (ai, ao) = (arm("immediate_follow", &imm), arm("other", &oth));
    let diff = if imm.len() as u64 >= MIN_TXS && oth.len() as u64 >= MIN_TXS {
        serde_json::json!(ai["coinbase_transfer_share"].as_f64().unwrap() - ao["coinbase_transfer_share"].as_f64().unwrap())
    } else {
        serde_json::Value::Null
    };
    let v = serde_json::json!({
        "ranges": [{
            "first_block": start, "last_block": last, "blocks": blocks,
            "liquidation_logs": logs.len(), "liquidation_blocks": liq_blocks.len(),
            "liquidation_txs": txs.len(),
            "abstain": diff.is_null(),
            "share_difference_immediate_minus_other": diff,
            "buckets": [ai, ao],
        }],
        "provenance": {
            "started_at": started_at,
            "seeds": [7],
            "command": format!("mainoracleimmediatecbsplit --chainstack-network {network} --start {start} --blocks {blocks} --out {}", out.display()),
            "hardware": format!("{} on {} logical cpus", std::env::consts::ARCH, std::thread::available_parallelism().map(|x| x.get()).unwrap_or(1)),
            "wall_s": started.elapsed().as_secs_f64(),
            "data_sources": [
                {"url": format!("chainstack:ethereum blocks {start} to {last} (logs, headers, traces)"),
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
    fn arm_abstains_below_minimum() {
        let a = arm("x", &[true; 5]);
        assert_eq!(a["abstain"], true);
        assert_eq!(a["coinbase_transfer_txs"], 5);
        let b = arm("y", &[]);
        assert!(b["coinbase_transfer_share"].is_null());
    }

    #[test]
    fn arm_share() {
        let mut r = vec![true; 10];
        r.extend(vec![false; 30]);
        let a = arm("z", &r);
        assert_eq!(a["coinbase_transfer_share"].as_f64().unwrap(), 0.25);
        assert_eq!(a["abstain"], false);
    }
}
