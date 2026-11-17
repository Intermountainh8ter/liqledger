//! `mainoraclesendercb`: among Aave V3 liquidation transactions, the share with a nonzero traced coinbase transfer for
//! the top-3 tx.from senders versus all other senders. Parameters are fixed in preregistration_additions.md. Reads only;
//! the endpoint is never printed and no sender address is written.

use crate::defcheck::chrono_now;
use crate::mainoracleimmediate::{chainstack_endpoint, liq_logs};
use crate::mainoracleimmediatecbshare::coinbase_value;
use alloy::providers::{Provider, ProviderBuilder};
use std::collections::BTreeMap;
use std::path::PathBuf;

const MIN_TXS: usize = 50;
const TRACE_CAP: usize = 600;

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

/// Splits (block, index, sender) rows into (top-3 senders by count, others), each in block and index order.
/// Ties in count break by sender string order.
pub fn split_top3<'a>(rows: &'a [(u64, u64, String)]) -> (Vec<&'a (u64, u64, String)>, Vec<&'a (u64, u64, String)>) {
    let mut counts: BTreeMap<&str, u64> = BTreeMap::new();
    for r in rows {
        *counts.entry(r.2.as_str()).or_insert(0) += 1;
    }
    let mut ranked: Vec<(&str, u64)> = counts.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    let top: Vec<&str> = ranked.iter().take(3).map(|x| x.0).collect();
    let mut sorted: Vec<&(u64, u64, String)> = rows.iter().collect();
    sorted.sort_by_key(|r| (r.0, r.1));
    sorted.into_iter().partition(|r| top.contains(&r.2.as_str()))
}

/// Share of `flags` that are true; None when empty.
fn share(flags: &[bool]) -> Option<f64> {
    if flags.is_empty() { None } else { Some(flags.iter().filter(|f| **f).count() as f64 / flags.len() as f64) }
}

fn summarize(top: &[bool], rest: &[bool], top_total: usize, rest_total: usize) -> serde_json::Value {
    let abstain = top_total < MIN_TXS || rest_total < MIN_TXS;
    let (t, r) = (share(top), share(rest));
    let diff = match (t, r) { (Some(t), Some(r)) if !abstain => Some(t - r), _ => None };
    serde_json::json!({
        "top3_txs": top_total, "rest_txs": rest_total, "top3_traced": top.len(), "rest_traced": rest.len(),
        "top3_cb_share": t, "rest_cb_share": r, "top3_minus_rest_cb_share": diff, "abstain": abstain,
    })
}

pub async fn mainoraclesendercb(network: String, start: u64, blocks: u64, out: PathBuf) -> anyhow::Result<()> {
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
    let mut txs: BTreeMap<(u64, u64), alloy::primitives::B256> = BTreeMap::new();
    for l in &logs {
        if let (Some(b), Some(i), Some(h)) = (l.block_number, l.transaction_index, l.transaction_hash) {
            txs.insert((b, i), h);
        }
    }
    let mut rows: Vec<(u64, u64, String)> = Vec::new();
    for ((b, i), h) in &txs {
        let r = raw(&p, "eth_getTransactionReceipt", serde_json::json!([format!("{h:#x}")])).await?;
        let from = r["from"].as_str().ok_or_else(|| anyhow::anyhow!("no from"))?.to_lowercase();
        rows.push((*b, *i, from));
    }
    let (top, rest) = split_top3(&rows);
    let mut heads: BTreeMap<u64, String> = BTreeMap::new();
    let keys = |group: &[&(u64, u64, String)]| -> Vec<(u64, u64)> { group.iter().take(TRACE_CAP).map(|r| (r.0, r.1)).collect() };
    let (top_keys, rest_keys) = (keys(&top), keys(&rest));
    let mut out_flags: [Vec<bool>; 2] = [Vec::new(), Vec::new()];
    for (g, keys) in [top_keys, rest_keys].into_iter().enumerate() {
        for (b, i) in keys {
            if !heads.contains_key(&b) {
                let hd = raw(&p, "eth_getBlockByNumber", serde_json::json!([format!("0x{b:x}"), false])).await?;
                heads.insert(b, hd["miner"].as_str().ok_or_else(|| anyhow::anyhow!("no miner"))?.to_string());
            }
            let tr = raw(&p, "trace_transaction", serde_json::json!([format!("{:#x}", txs[&(b, i)])])).await?;
            out_flags[g].push(coinbase_value(&tr, &heads[&b]) > 0);
        }
    }
    let mut range = summarize(&out_flags[0], &out_flags[1], top.len(), rest.len());
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
            "command": format!("mainoraclesendercb --chainstack-network {network} --start {start} --blocks {blocks} --out {}", out.display()),
            "hardware": format!("{} on {} logical cpus", std::env::consts::ARCH, std::thread::available_parallelism().map(|x| x.get()).unwrap_or(1)),
            "wall_s": started.elapsed().as_secs_f64(),
            "data_sources": [
                {"url": format!("chainstack:ethereum blocks {start} to {last} (logs, receipts, headers, traces)"),
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
    fn split_top3_ranks_by_count() {
        let mut rows = Vec::new();
        for (s, n) in [("a", 4u64), ("b", 3), ("c", 2), ("d", 1), ("e", 1)] {
            for k in 0..n { rows.push((k, rows.len() as u64, s.to_string())); }
        }
        let (top, rest) = split_top3(&rows);
        assert_eq!(top.len(), 9);
        assert_eq!(rest.len(), 2);
        assert!(rest.iter().all(|r| r.2 == "d" || r.2 == "e"));
    }

    #[test]
    fn summarize_diff_and_abstain() {
        let s = summarize(&[true, false], &[false, false], 60, 60);
        assert_eq!(s["top3_minus_rest_cb_share"].as_f64().unwrap(), 0.5);
        let s = summarize(&[true], &[false], 10, 60);
        assert_eq!(s["abstain"], true);
        assert!(s["top3_minus_rest_cb_share"].is_null());
    }
}
