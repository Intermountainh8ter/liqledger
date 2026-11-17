//! `mainoracleimmediatecbshare`: among Aave V3 liquidation transactions, the coinbase-transfer share of the total builder
//! payment (priority fee times gas used plus traced nonzero ETH value to the header coinbase). Parameters are fixed in
//! preregistration_additions.md. Reads only; the endpoint is never printed.

use crate::defcheck::chrono_now;
use crate::mainoracleimmediate::{chainstack_endpoint, liq_logs};
use alloy::providers::{Provider, ProviderBuilder};
use std::collections::BTreeMap;
use std::path::PathBuf;

const MIN_TXS: u64 = 20;

fn hex_u128(v: &serde_json::Value) -> Option<u128> {
    u128::from_str_radix(v.as_str()?.trim_start_matches("0x"), 16).ok()
}

/// Sum of nonzero values of non-failed calls whose `to` equals `miner`.
pub fn coinbase_value(traces: &serde_json::Value, miner: &str) -> u128 {
    traces.as_array().map_or(0, |a| {
        a.iter()
            .filter(|t| t["error"].is_null())
            .filter(|t| t["action"]["to"].as_str().is_some_and(|to| to.eq_ignore_ascii_case(miner)))
            .filter_map(|t| hex_u128(&t["action"]["value"]))
            .sum()
    })
}

/// Median of a slice (mean of the middle pair when even); None when empty.
pub fn median(v: &mut [f64]) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = v.len();
    Some(if n % 2 == 1 { v[n / 2] } else { (v[n / 2 - 1] + v[n / 2]) / 2.0 })
}

/// rows: (priority fee wei, coinbase transfer wei).
fn summarize(rows: &[(u128, u128)]) -> serde_json::Value {
    let paying: Vec<&(u128, u128)> = rows.iter().filter(|r| r.0 + r.1 > 0).collect();
    let mut shares: Vec<f64> = paying.iter().map(|r| r.1 as f64 / (r.0 + r.1) as f64).collect();
    let n = shares.len() as u64;
    let pure_cb = paying.iter().filter(|r| r.0 == 0).count();
    let no_cb = paying.iter().filter(|r| r.1 == 0).count();
    let med = if n >= MIN_TXS { median(&mut shares) } else { None };
    serde_json::json!({
        "liquidation_txs": rows.len(), "zero_payment_txs": rows.len() as u64 - n, "paying_txs": n,
        "coinbase_only_txs": pure_cb, "priority_fee_only_txs": no_cb,
        "coinbase_share_of_builder_payment_median": med,
        "abstain": n < MIN_TXS,
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

pub async fn mainoracleimmediatecbshare(network: String, start: u64, blocks: u64, out: PathBuf) -> anyhow::Result<()> {
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
    let mut heads: BTreeMap<u64, (String, u128)> = BTreeMap::new();
    let mut rows = Vec::new();
    for ((b, _), h) in &txs {
        if !heads.contains_key(b) {
            let hd = raw(&p, "eth_getBlockByNumber", serde_json::json!([format!("0x{b:x}"), false])).await?;
            let m = hd["miner"].as_str().ok_or_else(|| anyhow::anyhow!("no miner"))?.to_string();
            heads.insert(*b, (m, hex_u128(&hd["baseFeePerGas"]).ok_or_else(|| anyhow::anyhow!("no baseFeePerGas"))?));
        }
        let (miner, base) = &heads[b];
        let hs = format!("{h:#x}");
        let r = raw(&p, "eth_getTransactionReceipt", serde_json::json!([hs])).await?;
        let egp = hex_u128(&r["effectiveGasPrice"]).ok_or_else(|| anyhow::anyhow!("no effectiveGasPrice"))?;
        let gas = hex_u128(&r["gasUsed"]).ok_or_else(|| anyhow::anyhow!("no gasUsed"))?;
        let tr = raw(&p, "trace_transaction", serde_json::json!([hs])).await?;
        rows.push((egp.saturating_sub(*base) * gas, coinbase_value(&tr, miner)));
    }
    let mut range = summarize(&rows);
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
            "command": format!("mainoracleimmediatecbshare --chainstack-network {network} --start {start} --blocks {blocks} --out {}", out.display()),
            "hardware": format!("{} on {} logical cpus", std::env::consts::ARCH, std::thread::available_parallelism().map(|x| x.get()).unwrap_or(1)),
            "wall_s": started.elapsed().as_secs_f64(),
            "data_sources": [
                {"url": format!("chainstack:ethereum blocks {start} to {last} (logs, headers, receipts, traces)"),
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
    fn median_odd_even() {
        assert_eq!(median(&mut [3.0, 1.0, 2.0]), Some(2.0));
        assert_eq!(median(&mut [1.0, 2.0, 3.0, 4.0]), Some(2.5));
        assert_eq!(median(&mut []), None);
    }

    #[test]
    fn coinbase_value_sums_and_skips_errors() {
        let m = "0xAbC0000000000000000000000000000000000001";
        let t = serde_json::json!([
            {"action": {"to": m.to_lowercase(), "value": "0x5"}},
            {"action": {"to": m.to_lowercase(), "value": "0x7"}, "error": "Reverted"},
            {"action": {"to": "0x2", "value": "0x9"}}]);
        assert_eq!(coinbase_value(&t, m), 5);
        assert_eq!(coinbase_value(&serde_json::Value::Null, m), 0);
    }

    #[test]
    fn summarize_splits_zero_payment_and_abstains() {
        let mut rows = vec![(0u128, 0u128); 3];
        rows.extend(vec![(1u128, 3u128); 20]);
        let s = summarize(&rows);
        assert_eq!(s["zero_payment_txs"], 3);
        assert_eq!(s["paying_txs"], 20);
        assert_eq!(s["coinbase_share_of_builder_payment_median"].as_f64().unwrap(), 0.75);
        assert_eq!(s["abstain"], false);
        assert_eq!(summarize(&rows[..5])["abstain"], true);
    }
}
