//! `mainoracleimmediatecb`: among Aave V3 liquidation transactions directly after a Chainlink AnswerUpdated log, the
//! share with a call of nonzero ETH value to the block header coinbase (trace_transaction), split by zero versus nonzero
//! priority fee, with Wilson intervals. Parameters are fixed in preregistration_additions.md. Reads only; the endpoint
//! is never printed.

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

/// True when any call in a trace_transaction result has nonzero value and `to` equal to `miner` (lowercase hex).
pub fn pays_coinbase(traces: &serde_json::Value, miner: &str) -> bool {
    traces.as_array().is_some_and(|a| {
        a.iter().any(|t| {
            let act = &t["action"];
            act["to"].as_str().is_some_and(|to| to.eq_ignore_ascii_case(miner))
                && hex_u128(&act["value"]).is_some_and(|v| v > 0)
        })
    })
}

fn bucket(label: &str, rows: &[(bool, bool)]) -> serde_json::Value {
    // rows: (coinbase transfer, zero priority fee)
    let n = rows.len() as u64;
    let hit = rows.iter().filter(|r| r.0).count() as u64;
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

pub async fn mainoracleimmediatecb(network: String, start: u64, blocks: u64, out: PathBuf) -> anyhow::Result<()> {
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
            if i > 0 && upd.contains(&(b, i - 1)) {
                txs.insert((b, i), h);
            }
        }
    }
    let mut miners: BTreeMap<u64, (String, u128)> = BTreeMap::new();
    let mut rows = Vec::new();
    for ((b, _), h) in &txs {
        if !miners.contains_key(b) {
            let hd = raw(&p, "eth_getBlockByNumber", serde_json::json!([format!("0x{b:x}"), false])).await?;
            let m = hd["miner"].as_str().ok_or_else(|| anyhow::anyhow!("no miner"))?.to_string();
            miners.insert(*b, (m, hex_u128(&hd["baseFeePerGas"]).ok_or_else(|| anyhow::anyhow!("no baseFeePerGas"))?));
        }
        let (miner, base) = &miners[b];
        let hs = format!("{h:#x}");
        let r = raw(&p, "eth_getTransactionReceipt", serde_json::json!([hs])).await?;
        let egp = hex_u128(&r["effectiveGasPrice"]).ok_or_else(|| anyhow::anyhow!("no effectiveGasPrice"))?;
        let tr = raw(&p, "trace_transaction", serde_json::json!([hs])).await?;
        rows.push((pays_coinbase(&tr, miner), egp == *base));
    }
    let all = bucket("immediate_follow", &rows);
    let zero: Vec<_> = rows.iter().copied().filter(|r| r.1).collect();
    let nonzero: Vec<_> = rows.iter().copied().filter(|r| !r.1).collect();
    let v = serde_json::json!({
        "ranges": [{
            "first_block": start, "last_block": last, "blocks": blocks,
            "liquidation_logs": logs.len(), "liquidation_blocks": liq_blocks.len(),
            "immediate_follow_txs": rows.len(),
            "immediate_follow_coinbase_transfer_share": all["coinbase_transfer_share"].clone(),
            "buckets": [all, bucket("zero_priority_fee", &zero), bucket("nonzero_priority_fee", &nonzero)],
        }],
        "provenance": {
            "started_at": started_at,
            "seeds": [7],
            "command": format!("mainoracleimmediatecb --chainstack-network {network} --start {start} --blocks {blocks} --out {}", out.display()),
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
    fn detects_nonzero_value_to_miner() {
        let m = "0xAbC0000000000000000000000000000000000001";
        let t = serde_json::json!([
            {"action": {"to": "0x1", "value": "0x5"}},
            {"action": {"to": m.to_lowercase(), "value": "0x0"}}]);
        assert!(!pays_coinbase(&t, m));
        let t2 = serde_json::json!([{"action": {"to": m.to_lowercase(), "value": "0x1"}}]);
        assert!(pays_coinbase(&t2, m));
        assert!(!pays_coinbase(&serde_json::Value::Null, m));
    }
}
