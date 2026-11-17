//! `mainoracleimmediate`: on Ethereum mainnet, share of Aave V3 liquidation transactions (in blocks that also hold a
//! Chainlink AnswerUpdated log) sitting at the transaction index directly after an update transaction. Parameters are fixed in preregistration_additions.md. Reads only; the endpoint
//! is resolved from the Chainstack platform API at run time and is never printed.

use crate::defcheck::chrono_now;
use crate::{addr, AnswerUpdated, LiquidationCall};
use alloy::providers::{Provider, ProviderBuilder};
use alloy::rpc::types::{Filter, Log};
use alloy::sol_types::SolEvent;
use std::collections::BTreeSet;
use std::path::PathBuf;

use crate::AAVE_V3_POOL as BASE_AAVE_V3_POOL;
const MIN_TXS: u64 = 20;

/// Liquidation transactions (deduplicated) in blocks holding an update: (block, tx index, immediately follows an update).
pub fn immediate(liqs: &[(u64, u64)], updates: &[(u64, u64)]) -> Vec<(u64, u64, bool)> {
    let upd: BTreeSet<(u64, u64)> = updates.iter().copied().collect();
    let blocks: BTreeSet<u64> = updates.iter().map(|u| u.0).collect();
    let txs: BTreeSet<(u64, u64)> = liqs.iter().copied().filter(|l| blocks.contains(&l.0)).collect();
    txs.into_iter().map(|(b, i)| (b, i, i > 0 && upd.contains(&(b, i - 1)))).collect()
}

fn pos(logs: &[Log]) -> Vec<(u64, u64)> {
    logs.iter().filter_map(|l| Some((l.block_number?, l.transaction_index?))).collect()
}

async fn logs_retry(p: &impl Provider, f: &Filter) -> anyhow::Result<Vec<Log>> {
    let mut tries = 0;
    loop {
        match p.get_logs(f).await {
            Ok(v) => return Ok(v),
            Err(_) if tries < 5 => {
                tries += 1;
                tokio::time::sleep(std::time::Duration::from_millis(500 * (1 << tries))).await;
            }
            Err(_) => anyhow::bail!("ethereum eth_getLogs failed"),
        }
    }
}

/// Liquidation logs over [from, to], halving the span when the node refuses a range.
async fn liq_logs(p: &impl Provider, from: u64, to: u64) -> anyhow::Result<Vec<Log>> {
    let f = Filter::new()
        .address(addr(BASE_AAVE_V3_POOL))
        .event_signature(LiquidationCall::SIGNATURE_HASH)
        .from_block(from)
        .to_block(to);
    match logs_retry(p, &f).await {
        Ok(v) => Ok(v),
        Err(_) if to > from => {
            let mid = from + (to - from) / 2;
            let mut a = Box::pin(liq_logs(p, from, mid)).await?;
            a.extend(Box::pin(liq_logs(p, mid + 1, to)).await?);
            Ok(a)
        }
        Err(e) => Err(e),
    }
}

/// Pick the https endpoint of the first node on `network` from a platform API node list.
pub fn pick_endpoint(list: &serde_json::Value, network: &str) -> Option<String> {
    list["results"].as_array()?.iter().find_map(|n| {
        let net = n["network"].as_str().unwrap_or("").to_lowercase();
        let nm = n["name"].as_str().unwrap_or("").to_lowercase();
        let d = &n["details"];
        let (h, k) = (d["https_endpoint"].as_str()?, d["auth_key"].as_str().unwrap_or(""));
        let ok = (net.contains(network) || nm.contains(network)) && !net.contains("sepolia") && !net.contains("holesky") && !net.contains("hoodi");
        ok.then(|| if k.is_empty() { h.to_string() } else { format!("{}/{}", h.trim_end_matches('/'), k) })
    })
}

/// Resolve the endpoint from the platform API at run time; errors name the network and method only.
fn chainstack_endpoint(network: &str) -> anyhow::Result<String> {
    let key = std::env::var("CHAINSTACK_API_KEY").map_err(|_| anyhow::anyhow!("CHAINSTACK_API_KEY not set"))?;
    let out = std::process::Command::new("curl")
        .args(["-sS", "-f", "-A", "liqledger/0.1 curl", "-H", &format!("Authorization: Bearer {key}"), "https://api.chainstack.com/v1/nodes/?limit=100"])
        .output()
        .map_err(|_| anyhow::anyhow!("chainstack GET /v1/nodes/ could not start curl"))?;
    anyhow::ensure!(out.status.success(), "chainstack GET /v1/nodes/ failed");
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).map_err(|_| anyhow::anyhow!("chainstack GET /v1/nodes/ bad json"))?;
    pick_endpoint(&v, network).ok_or_else(|| anyhow::anyhow!("no {network} node in chainstack account"))
}

pub async fn mainoracleimmediate(network: String, start: u64, blocks: u64, out: PathBuf) -> anyhow::Result<()> {
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
    let m = immediate(&liqs, &updates);
    let n = m.len() as u64;
    let hit = m.iter().filter(|x| x.2).count() as u64;
    let share = if n > 0 { serde_json::json!(hit as f64 / n as f64) } else { serde_json::Value::Null };
    let v = serde_json::json!({
        "ranges": [{
            "first_block": start, "last_block": last, "blocks": blocks,
            "liquidation_logs": liqs.len(), "liquidation_blocks": liq_blocks.len(),
            "same_block_liquidation_txs": n,
            "immediate_follow_txs": hit,
            "immediate_follow_share": share,
            "abstain": n < MIN_TXS,
        }],
        "provenance": {
            "started_at": started_at,
            "seeds": [7],
            "command": format!("mainoracleimmediate --chainstack-network {network} --start {start} --blocks {blocks} --out {}", out.display()),
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
    fn pick_endpoint_joins_key() {
        let v = serde_json::json!({"results": [
            {"network": "ethereum-sepolia", "name": "x", "details": {"https_endpoint": "https://a", "auth_key": "k0"}},
            {"network": "ethereum-mainnet", "name": "n", "details": {"https_endpoint": "https://b/", "auth_key": "k1"}}]});
        assert_eq!(pick_endpoint(&v, "ethereum").as_deref(), Some("https://b/k1"));
        assert_eq!(pick_endpoint(&v, "base"), None);
    }

    #[test]
    fn immediate_marks_next_index() {
        let r = immediate(&[(10, 5), (10, 5), (10, 9), (11, 3), (12, 0)], &[(10, 4), (11, 7), (12, 0)]);
        assert_eq!(r, vec![(10, 5, true), (10, 9, false), (11, 3, false), (12, 0, false)]);
    }
}
