//! `mainoraclesendershare`: concentration of Aave V3 liquidation transactions by tx.from sender, reported as ranks and
//! shares only (no addresses). Parameters are fixed in preregistration_additions.md. Reads only; the endpoint is never printed.

use crate::defcheck::chrono_now;
use crate::mainoracleimmediate::{chainstack_endpoint, liq_logs};
use alloy::providers::{Provider, ProviderBuilder};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

const MIN_TXS: u64 = 50;

/// Share of the `k` most frequent senders among all transactions; counts per sender in any order.
pub fn top_k_share(counts: &[u64], k: usize) -> f64 {
    let mut c = counts.to_vec();
    c.sort_unstable_by(|a, b| b.cmp(a));
    let total: u64 = c.iter().sum();
    if total == 0 {
        return 0.0;
    }
    c.iter().take(k).sum::<u64>() as f64 / total as f64
}

fn summarize(counts: &[u64]) -> serde_json::Value {
    let n: u64 = counts.iter().sum();
    let abstain = n < MIN_TXS;
    let share = |k| if abstain { None } else { Some(top_k_share(counts, k)) };
    serde_json::json!({
        "liquidation_txs": n, "distinct_senders": counts.len(),
        "top1_sender_share": share(1), "top3_sender_share": share(3),
        "abstain": abstain,
    })
}

async fn receipt(p: &impl Provider, hash: &str) -> anyhow::Result<serde_json::Value> {
    let mut tries = 0;
    loop {
        match p.raw_request::<_, serde_json::Value>("eth_getTransactionReceipt".into(), serde_json::json!([hash])).await {
            Ok(v) if !v.is_null() => return Ok(v),
            _ if tries < 5 => {
                tries += 1;
                tokio::time::sleep(std::time::Duration::from_millis(500 * (1 << tries))).await;
            }
            _ => anyhow::bail!("ethereum eth_getTransactionReceipt failed"),
        }
    }
}

pub async fn mainoraclesendershare(network: String, start: u64, blocks: u64, out: PathBuf) -> anyhow::Result<()> {
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
    let mut txs: BTreeSet<(u64, u64, alloy::primitives::B256)> = BTreeSet::new();
    for l in &logs {
        if let (Some(b), Some(i), Some(h)) = (l.block_number, l.transaction_index, l.transaction_hash) {
            txs.insert((b, i, h));
        }
    }
    let mut by_sender: BTreeMap<String, u64> = BTreeMap::new();
    for (_, _, h) in &txs {
        let r = receipt(&p, &format!("{h:#x}")).await?;
        let from = r["from"].as_str().ok_or_else(|| anyhow::anyhow!("no from"))?.to_lowercase();
        *by_sender.entry(from).or_insert(0) += 1;
    }
    let counts: Vec<u64> = by_sender.values().copied().collect();
    let mut range = summarize(&counts);
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
            "command": format!("mainoraclesendershare --chainstack-network {network} --start {start} --blocks {blocks} --out {}", out.display()),
            "hardware": format!("{} on {} logical cpus", std::env::consts::ARCH, std::thread::available_parallelism().map(|x| x.get()).unwrap_or(1)),
            "wall_s": started.elapsed().as_secs_f64(),
            "data_sources": [
                {"url": format!("chainstack:ethereum blocks {start} to {last} (logs, receipts)"),
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
    fn top_k_share_orders_counts() {
        assert_eq!(top_k_share(&[1, 5, 2, 2], 1), 0.5);
        assert_eq!(top_k_share(&[1, 5, 2, 2], 3), 0.9);
        assert_eq!(top_k_share(&[], 3), 0.0);
    }

    #[test]
    fn summarize_abstains_below_minimum() {
        let s = summarize(&[10, 10]);
        assert_eq!(s["abstain"], true);
        assert!(s["top3_sender_share"].is_null());
        let s = summarize(&[30, 20, 10, 10]);
        assert_eq!(s["abstain"], false);
        assert_eq!(s["top3_sender_share"].as_f64().unwrap(), 60.0 / 70.0);
    }
}
