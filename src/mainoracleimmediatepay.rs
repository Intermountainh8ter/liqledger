//! `mainoracleimmediatepay`: among Aave V3 liquidation transactions in blocks that also hold a Chainlink AnswerUpdated
//! log, the share with an exactly zero priority fee (effectiveGasPrice equal to the block base fee), for transactions
//! directly after an update versus the others, with Wilson intervals. Parameters are fixed in
//! preregistration_additions.md. Reads only; the endpoint is never printed.

use crate::defcheck::chrono_now;
use crate::mainoracleimmediate::{chainstack_endpoint, liq_logs, logs_retry, pos};
use crate::mainoracleimmediatesplit::wilson;
use crate::AnswerUpdated;
use alloy::providers::{Provider, ProviderBuilder};
use alloy::rpc::types::Filter;
use alloy::sol_types::SolEvent;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

const MIN_TXS: u64 = 20;

fn hex_u128(v: &serde_json::Value) -> Option<u128> {
    u128::from_str_radix(v.as_str()?.trim_start_matches("0x"), 16).ok()
}

/// (bucket 0 = immediate follow, 1 = other, zero priority fee) per transaction.
/// Input: (block, tx index, effective gas price, block base fee); `updates` are (block, tx index) of AnswerUpdated logs.
pub fn pay_split(txs: &[(u64, u64, u128, u128)], updates: &[(u64, u64)]) -> Vec<(usize, bool)> {
    let upd: BTreeSet<(u64, u64)> = updates.iter().copied().collect();
    txs.iter()
        .map(|t| {
            let imm = t.1 > 0 && upd.contains(&(t.0, t.1 - 1));
            (usize::from(!imm), t.2 == t.3)
        })
        .collect()
}

fn bucket(label: &str, rows: &[(usize, bool)], idx: usize) -> serde_json::Value {
    let n = rows.iter().filter(|r| r.0 == idx).count() as u64;
    let hit = rows.iter().filter(|r| r.0 == idx && r.1).count() as u64;
    let ci = wilson(hit, n);
    let share = if n > 0 { serde_json::json!(hit as f64 / n as f64) } else { serde_json::Value::Null };
    serde_json::json!({
        "label": label, "txs": n, "zero_priority_txs": hit, "zero_priority_share": share,
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

pub async fn mainoracleimmediatepay(network: String, start: u64, blocks: u64, out: PathBuf) -> anyhow::Result<()> {
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
    let upd_blocks: BTreeSet<u64> = updates.iter().map(|u| u.0).collect();
    // Distinct liquidation transactions in update blocks, keyed by (block, index).
    let mut txs: BTreeMap<(u64, u64), alloy::primitives::B256> = BTreeMap::new();
    for l in &logs {
        if let (Some(b), Some(i), Some(h)) = (l.block_number, l.transaction_index, l.transaction_hash) {
            if upd_blocks.contains(&b) {
                txs.insert((b, i), h);
            }
        }
    }
    let mut base: BTreeMap<u64, u128> = BTreeMap::new();
    for b in &upd_blocks {
        if txs.keys().any(|k| k.0 == *b) {
            let h = raw(&p, "eth_getBlockByNumber", serde_json::json!([format!("0x{b:x}"), false])).await?;
            base.insert(*b, hex_u128(&h["baseFeePerGas"]).ok_or_else(|| anyhow::anyhow!("no baseFeePerGas"))?);
        }
    }
    let mut rows = Vec::new();
    for ((b, i), h) in &txs {
        let r = raw(&p, "eth_getTransactionReceipt", serde_json::json!([format!("{h:#x}")])).await?;
        let egp = hex_u128(&r["effectiveGasPrice"]).ok_or_else(|| anyhow::anyhow!("no effectiveGasPrice"))?;
        rows.push((*b, *i, egp, base[b]));
    }
    let m = pay_split(&rows, &updates);
    let imm = bucket("immediate_follow", &m, 0);
    let v = serde_json::json!({
        "ranges": [{
            "first_block": start, "last_block": last, "blocks": blocks,
            "liquidation_logs": logs.len(), "update_blocks_with_liquidations": upd_blocks.len(),
            "same_block_liquidation_txs": rows.len(),
            "immediate_follow_zero_priority_share": imm["zero_priority_share"].clone(),
            "buckets": [imm, bucket("other_same_block", &m, 1)],
        }],
        "provenance": {
            "started_at": started_at,
            "seeds": [7],
            "command": format!("mainoracleimmediatepay --chainstack-network {network} --start {start} --blocks {blocks} --out {}", out.display()),
            "hardware": format!("{} on {} logical cpus", std::env::consts::ARCH, std::thread::available_parallelism().map(|x| x.get()).unwrap_or(1)),
            "wall_s": started.elapsed().as_secs_f64(),
            "data_sources": [
                {"url": format!("chainstack:ethereum blocks {start} to {last} (logs, headers, receipts)"),
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
    fn pay_split_marks_immediate_and_zero_priority() {
        let txs = [(10, 5, 100, 100), (10, 9, 120, 100), (10, 0, 100, 100), (11, 3, 100, 100)];
        let r = pay_split(&txs, &[(10, 4), (10, 8)]);
        assert_eq!(r, vec![(0, true), (0, false), (1, true), (1, true)]);
    }

    #[test]
    fn hex_parses() {
        assert_eq!(hex_u128(&serde_json::json!("0x10")), Some(16));
        assert_eq!(hex_u128(&serde_json::Value::Null), None);
    }
}
