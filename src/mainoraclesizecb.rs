//! `mainoraclesizecb`: among single-log Aave V3 USDC/USDT-debt liquidation transactions, the share with a nonzero traced
//! coinbase transfer for debtToCover below versus at least 1000 units. Parameters are fixed in
//! preregistration_additions.md. Reads only; the endpoint is never printed.

use crate::defcheck::chrono_now;
use crate::mainoracleimmediate::{chainstack_endpoint, liq_logs};
use crate::mainoracleimmediatecbshare::coinbase_value;
use crate::mainoracleimmediatesize::stable_debt;
use alloy::providers::{Provider, ProviderBuilder};
use std::collections::BTreeMap;
use std::path::PathBuf;

const MIN_TXS: usize = 30;
const TRACE_CAP: usize = 600;
/// 1000 units of a 6-decimal stablecoin.
const THRESHOLD_RAW: u128 = 1_000_000_000;

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

/// Single-log stable-debt rows (block, index, bucket 0 small / 1 large) in block and index order.
/// `all_counts` holds every LiquidationCall log position so multi-log transactions are excluded.
pub fn buckets(stable: &[(u64, u64, u128)], all_counts: &[(u64, u64)]) -> Vec<(u64, u64, usize)> {
    let mut cnt: BTreeMap<(u64, u64), u64> = BTreeMap::new();
    for l in all_counts {
        *cnt.entry(*l).or_default() += 1;
    }
    let mut v: Vec<(u64, u64, usize)> = stable
        .iter()
        .filter(|s| cnt.get(&(s.0, s.1)) == Some(&1))
        .map(|s| (s.0, s.1, usize::from(s.2 >= THRESHOLD_RAW)))
        .collect();
    v.sort();
    v
}

fn share(flags: &[bool]) -> Option<f64> {
    if flags.is_empty() { None } else { Some(flags.iter().filter(|f| **f).count() as f64 / flags.len() as f64) }
}

fn summarize(small: &[bool], large: &[bool], small_total: usize, large_total: usize) -> serde_json::Value {
    let abstain = small_total < MIN_TXS || large_total < MIN_TXS;
    let (s, l) = (share(small), share(large));
    let diff = match (s, l) { (Some(s), Some(l)) if !abstain => Some(l - s), _ => None };
    serde_json::json!({
        "small_txs": small_total, "large_txs": large_total, "small_traced": small.len(), "large_traced": large.len(),
        "small_cb_share": s, "large_cb_share": l, "large_minus_small_cb_share": diff, "abstain": abstain,
    })
}

pub async fn mainoraclesizecb(network: String, start: u64, blocks: u64, out: PathBuf) -> anyhow::Result<()> {
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
    let mut hashes: BTreeMap<(u64, u64), alloy::primitives::B256> = BTreeMap::new();
    let mut all: Vec<(u64, u64)> = Vec::new();
    for l in &logs {
        if let (Some(b), Some(i), Some(h)) = (l.block_number, l.transaction_index, l.transaction_hash) {
            hashes.insert((b, i), h);
            all.push((b, i));
        }
    }
    let rows = buckets(&stable_debt(&logs), &all);
    let group = |g: usize| -> Vec<(u64, u64)> { rows.iter().filter(|r| r.2 == g).map(|r| (r.0, r.1)).collect() };
    let (small, large) = (group(0), group(1));
    let mut heads: BTreeMap<u64, String> = BTreeMap::new();
    let mut flags: [Vec<bool>; 2] = [Vec::new(), Vec::new()];
    for (g, keys) in [&small, &large].into_iter().enumerate() {
        for (b, i) in keys.iter().take(TRACE_CAP) {
            if !heads.contains_key(b) {
                let hd = raw(&p, "eth_getBlockByNumber", serde_json::json!([format!("0x{b:x}"), false])).await?;
                heads.insert(*b, hd["miner"].as_str().ok_or_else(|| anyhow::anyhow!("no miner"))?.to_string());
            }
            let tr = raw(&p, "trace_transaction", serde_json::json!([format!("{:#x}", hashes[&(*b, *i)])])).await?;
            flags[g].push(coinbase_value(&tr, &heads[b]) > 0);
        }
    }
    let mut range = summarize(&flags[0], &flags[1], small.len(), large.len());
    let o = range.as_object_mut().unwrap();
    o.insert("first_block".into(), start.into());
    o.insert("last_block".into(), last.into());
    o.insert("blocks".into(), blocks.into());
    o.insert("liquidation_logs".into(), logs.len().into());
    o.insert("threshold_units".into(), 1000.into());
    let v = serde_json::json!({
        "ranges": [range],
        "provenance": {
            "started_at": started_at,
            "seeds": [7],
            "command": format!("mainoraclesizecb --chainstack-network {network} --start {start} --blocks {blocks} --out {}", out.display()),
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
    fn buckets_single_log_and_threshold() {
        let stable = [(10, 5, 2_000_000_000u128), (10, 9, 5), (10, 7, 1_000_000_000), (11, 3, 999_999_999)];
        let all = [(10, 5), (10, 9), (10, 9), (10, 7), (11, 3)];
        assert_eq!(buckets(&stable, &all), vec![(10, 5, 1), (10, 7, 1), (11, 3, 0)]);
    }

    #[test]
    fn summarize_diff_and_abstain() {
        let s = summarize(&[false, false], &[true, false], 40, 40);
        assert_eq!(s["large_minus_small_cb_share"].as_f64().unwrap(), 0.5);
        let s = summarize(&[true], &[false], 10, 60);
        assert_eq!(s["abstain"], true);
        assert!(s["large_minus_small_cb_share"].is_null());
    }
}
