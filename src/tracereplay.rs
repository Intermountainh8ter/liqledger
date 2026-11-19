//! `tracereplay`: re-execute the first N Aave V3 LiquidationCall transactions of a pinned window with debug_traceCall
//! at the parent block (earlier same-block transactions not applied) and compare success status and LiquidationCall
//! log count with the landed receipt. Parameters are fixed in preregistration_additions.md. Reads only; the endpoint
//! is never printed and no address is written.

use crate::defcheck::chrono_now;
use crate::mainoracleimmediate::{chainstack_endpoint, liq_logs};
use crate::LiquidationCall;
use alloy::providers::{Provider, ProviderBuilder};
use alloy::sol_types::SolEvent;
use std::path::PathBuf;


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

/// Number of LiquidationCall logs in a callTracer result (withLog) or receipt log array, by topic0.
pub fn count_liq(logs: &serde_json::Value) -> usize {
    logs.as_array()
        .map(|a| {
            a.iter()
                .filter(|l| {
                    let t0 = l["topics"].get(0).and_then(|t| t.as_str()).unwrap_or("");
                    t0.eq_ignore_ascii_case(&format!("{:#x}", LiquidationCall::SIGNATURE_HASH))
                })
                .count()
        })
        .unwrap_or(0)
}

/// Collects logs from every call frame of a callTracer (withLog) result, depth first.
pub fn trace_logs(frame: &serde_json::Value, out: &mut Vec<serde_json::Value>) {
    if let Some(a) = frame["logs"].as_array() {
        out.extend(a.iter().cloned());
    }
    if let Some(c) = frame["calls"].as_array() {
        for f in c {
            trace_logs(f, out);
        }
    }
}

/// A replay is exact when success status and LiquidationCall count both match the receipt.
pub fn exact(receipt_ok: bool, receipt_liq: usize, replay_ok: bool, replay_liq: usize) -> bool {
    receipt_ok == replay_ok && receipt_liq == replay_liq
}

/// (index-zero replayed, index-zero failed, nonzero replayed, nonzero failed) from (tx index, exact) rows.
pub fn split_by_index(rows: &[(u64, bool)]) -> (u64, u64, u64, u64) {
    let mut r = (0, 0, 0, 0);
    for (i, e) in rows {
        if *i == 0 {
            r.0 += 1;
            r.1 += !e as u64;
        } else {
            r.2 += 1;
            r.3 += !e as u64;
        }
    }
    r
}

/// Block override for debug_traceCall built from the landed block's header.
fn block_override(h: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "number": h["number"], "time": h["timestamp"], "baseFeePerGas": h["baseFeePerGas"],
        "feeRecipient": h["miner"], "gasLimit": h["gasLimit"], "prevRandao": h["mixHash"],
    })
}

pub async fn tracereplay(network: String, start: u64, end: u64, max_txs: usize, seed: u64, mode: String, out: PathBuf) -> anyhow::Result<()> {
    anyhow::ensure!(mode == "trace-call" || mode == "trace-transaction" || mode == "trace-call-ctx", "mode must be trace-call, trace-call-ctx or trace-transaction");
    let by_tx = mode == "trace-transaction";
    let started = std::time::Instant::now();
    let started_at = chrono_now();
    anyhow::ensure!(end >= start, "end block before start block");
    let url = chainstack_endpoint(&network)?;
    let p = ProviderBuilder::new().connect_http(url.parse().map_err(|_| anyhow::anyhow!("bad {network} endpoint"))?);
    let mut logs = Vec::new();
    let mut a = start;
    while a <= end {
        let z = (a + 9_999).min(end);
        logs.extend(liq_logs(&p, a, z).await?);
        a = z + 1;
    }
    let mut txs: Vec<(u64, u64, alloy::primitives::B256)> = logs
        .iter()
        .filter_map(|l| Some((l.block_number?, l.transaction_index?, l.transaction_hash?)))
        .collect();
    txs.sort();
    txs.dedup_by_key(|t| t.2);
    let liq_txs_in_window = txs.len();
    txs.truncate(max_txs);
    let (mut n, mut fail, mut status_diff, mut count_diff, mut trace_err) = (0u64, 0u64, 0u64, 0u64, 0u64);
    let mut nonzero_index = 0u64;
    let mut rows: Vec<(u64, bool)> = Vec::new();
    for (b, ti, h) in &txs {
        if *ti > 0 {
            nonzero_index += 1;
        }
        let hs = format!("{h:#x}");
        let rc = raw(&p, "eth_getTransactionReceipt", serde_json::json!([hs])).await?;
        let tx = raw(&p, "eth_getTransactionByHash", serde_json::json!([hs])).await?;
        let r_ok = rc["status"].as_str() == Some("0x1");
        let r_liq = count_liq(&rc["logs"]);
        let call = serde_json::json!({
            "from": tx["from"], "to": tx["to"], "gas": tx["gas"], "value": tx["value"], "data": tx["input"],
        });
        let mut cfg = serde_json::json!({"tracer": "callTracer", "tracerConfig": {"withLog": true}});
        if mode == "trace-call-ctx" {
            let hd = raw(&p, "eth_getBlockByNumber", serde_json::json!([format!("0x{:x}", b), false])).await?;
            cfg["blockOverrides"] = block_override(&hd);
        }
        let tr = if by_tx {
            raw(&p, "debug_traceTransaction", serde_json::json!([hs, cfg])).await
        } else {
            raw(&p, "debug_traceCall", serde_json::json!([call, format!("0x{:x}", b - 1), cfg])).await
        };
        n += 1;
        let fail_before = fail;
        match tr {
            Ok(t) => {
                let ok = t["error"].is_null();
                let mut ls = Vec::new();
                if ok {
                    trace_logs(&t, &mut ls);
                }
                let liq = count_liq(&serde_json::Value::Array(ls));
                if ok != r_ok {
                    status_diff += 1;
                }
                if liq != r_liq {
                    count_diff += 1;
                }
                if !exact(r_ok, r_liq, ok, liq) {
                    fail += 1;
                }
            }
            Err(_) => {
                trace_err += 1;
                fail += 1;
            }
        }
        rows.push((*ti, fail == fail_before));
    }
    let (zero_n, zero_fail, nonzero_n, nonzero_fail) = split_by_index(&rows);
    let per_tx: Vec<serde_json::Value> = rows.iter().map(|(i, e)| serde_json::json!({"tx_index": i, "exact": e})).collect();
    let last = end;
    let fail_key = if by_tx { "exact_fail_count_trace_transaction" } else { "exact_fail_count" };
    let v = serde_json::json!({
        fail_key: fail,
        "mode": mode,
        "nonzero_tx_index_count": nonzero_index,
        "replayed_n": n,
        "exact_fail_count_index_zero": zero_fail,
        "replayed_n_index_zero": zero_n,
        "exact_fail_count_index_nonzero": nonzero_fail,
        "replayed_n_index_nonzero": nonzero_n,
        "per_tx": per_tx,
        "exact_match_count": n - fail,
        "status_diff_count": status_diff,
        "liq_count_diff_count": count_diff,
        "trace_error_count": trace_err,
        "liquidation_txs_in_window": liq_txs_in_window,
        "first_block": start,
        "last_block": end,
        "max_txs": max_txs,
        "provenance": {
            "started_at": started_at,
            "seeds": [seed],
            "command": format!("tracereplay --chainstack-network {network} --start-block {start} --end-block {end} --max-txs {max_txs} --seed {seed} --mode {mode} --out {}", out.display()),
            "hardware": format!("{} on {} logical cpus", std::env::consts::ARCH, std::thread::available_parallelism().map(|x| x.get()).unwrap_or(1)),
            "wall_s": started.elapsed().as_secs_f64(),
            "data_sources": [
                {"url": format!("chainstack:{network} blocks {start} to {last} (logs, receipts, {})", if by_tx { "debug_traceTransaction" } else { "debug_traceCall at parent block" }),
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
    fn counts_liq_topics_case_insensitively() {
        let l = serde_json::json!([{"topics": [format!("{:#x}", LiquidationCall::SIGNATURE_HASH).to_uppercase().replace("0X", "0x")]}, {"topics": ["0xabc"]}, {"topics": []}]);
        assert_eq!(count_liq(&l), 1);
        assert_eq!(count_liq(&serde_json::json!(null)), 0);
    }

    #[test]
    fn block_override_maps_header_fields() {
        let h = serde_json::json!({"number": "0x1", "timestamp": "0x2", "baseFeePerGas": "0x3", "miner": "0x4", "gasLimit": "0x5", "mixHash": "0x6"});
        let o = block_override(&h);
        assert_eq!(o["time"], "0x2");
        assert_eq!(o["feeRecipient"], "0x4");
        assert_eq!(o["prevRandao"], "0x6");
    }

    #[test]
    fn collects_nested_logs() {
        let t = serde_json::json!({"logs": [{"a": 1}], "calls": [{"logs": [{"a": 2}], "calls": [{"logs": [{"a": 3}]}]}, {}]});
        let mut o = Vec::new();
        trace_logs(&t, &mut o);
        assert_eq!(o.len(), 3);
    }

    #[tokio::test]
    async fn rejects_unknown_mode() {
        let r = tracereplay("ethereum".into(), 2, 1, 1, 7, "bogus".into(), PathBuf::from("/nonexistent/x.json")).await;
        assert!(r.unwrap_err().to_string().contains("mode must be"));
    }

    #[test]
    fn splits_by_index() {
        assert_eq!(split_by_index(&[(0, true), (0, false), (3, false), (5, true), (7, false)]), (2, 1, 3, 2));
        assert_eq!(split_by_index(&[]), (0, 0, 0, 0));
    }

    #[test]
    fn exact_needs_status_and_count() {
        assert!(exact(true, 1, true, 1));
        assert!(!exact(true, 1, false, 0));
        assert!(!exact(true, 1, true, 2));
    }
}
