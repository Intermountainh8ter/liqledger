//! `v4reexec`: replay each distinct Aave V4 liquidation transaction of a pinned range with debug_traceCall at the parent block
//! and compare the ordered debtAmountRestored values of the replayed LiquidationCall logs with the landed ones.
//! Parameters are fixed in preregistration_additions.md. Reads only; the endpoint is never printed.

use crate::defcheck::chrono_now;
use crate::mainoracleimmediate::{chainstack_endpoint, logs_retry};
use crate::mainoracleimmediatesplit::wilson;
use crate::tracereplay::{raw, trace_logs};
use crate::v4census::{fetch_source, parse_spokes, LiquidationCall};
use alloy::primitives::{Address, Bytes, B256, U256};
use alloy::providers::ProviderBuilder;
use alloy::rpc::types::Filter;
use alloy::sol_types::SolEvent;
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Ordered debtAmountRestored values of the V4 LiquidationCall logs in a callTracer log array emitted by one of `spokes`.
pub fn replay_amounts(logs: &[serde_json::Value], spokes: &[Address]) -> Vec<U256> {
    let mut out = Vec::new();
    for l in logs {
        let Some(a) = l["address"].as_str().and_then(|s| s.parse::<Address>().ok()) else { continue };
        if !spokes.contains(&a) {
            continue;
        }
        let topics: Vec<B256> = l["topics"].as_array().map(|t| t.iter().filter_map(|x| x.as_str()?.parse().ok()).collect()).unwrap_or_default();
        let data: Bytes = l["data"].as_str().and_then(|s| s.parse().ok()).unwrap_or_default();
        let Some(log) = alloy::primitives::LogData::new(topics, data) else { continue };
        if let Ok(ev) = LiquidationCall::decode_log(&alloy::primitives::Log { address: a, data: log }) {
            out.push(ev.data.debtAmountRestored);
        }
    }
    out
}

/// (replayed, mismatched, index-zero replayed, index-zero mismatched, nonzero replayed, nonzero mismatched) from (tx index, mismatch) rows.
pub fn split(rows: &[(u64, bool)]) -> (u64, u64, u64, u64, u64, u64) {
    let mut r = (0, 0, 0, 0, 0, 0);
    for (i, m) in rows {
        r.0 += 1;
        r.1 += *m as u64;
        if *i == 0 {
            r.2 += 1;
            r.3 += *m as u64;
        } else {
            r.4 += 1;
            r.5 += *m as u64;
        }
    }
    r
}

fn share(k: u64, n: u64) -> Option<f64> {
    (n > 0).then(|| k as f64 / n as f64)
}

pub async fn v4reexec(network: String, start: u64, blocks: u64, commit: String, seed: u64, out: PathBuf) -> anyhow::Result<()> {
    let started = std::time::Instant::now();
    let started_at = chrono_now();
    anyhow::ensure!(blocks >= 1, "blocks must be at least 1");
    let last = start + blocks - 1;
    let found = parse_spokes(&fetch_source(&commit)?);
    let spokes: Vec<Address> = found.iter().filter_map(|(_, a)| a.parse().ok()).collect();
    anyhow::ensure!(!spokes.is_empty(), "no Spokes found at the pinned commit");
    let url = chainstack_endpoint(&network)?;
    let p = ProviderBuilder::new().connect_http(url.parse().map_err(|_| anyhow::anyhow!("bad {network} endpoint"))?);
    // tx hash -> (block, first tx index, landed amounts in log order)
    let mut txs: BTreeMap<B256, (u64, u64, Vec<(u64, U256)>)> = BTreeMap::new();
    let mut a = start;
    while a <= last {
        let z = (a + 9_999).min(last);
        let f = Filter::new().address(spokes.clone()).event_signature(LiquidationCall::SIGNATURE_HASH).from_block(a).to_block(z);
        for l in logs_retry(&p, &f).await? {
            let (Some(h), Some(b), Some(i), Some(li)) = (l.transaction_hash, l.block_number, l.transaction_index, l.log_index) else {
                anyhow::bail!("ethereum eth_getLogs returned a log without position fields");
            };
            let ev = LiquidationCall::decode_log(&l.inner).map_err(|_| anyhow::anyhow!("ethereum eth_getLogs returned an undecodable LiquidationCall"))?;
            let e = txs.entry(h).or_insert((b, i, Vec::new()));
            e.1 = e.1.min(i);
            e.2.push((li, ev.data.debtAmountRestored));
        }
        a = z + 1;
    }
    let (mut failed, mut count_diff) = (0u64, 0u64);
    let mut rows: Vec<(u64, bool)> = Vec::new();
    for (h, (b, ti, landed)) in txs.iter_mut() {
        landed.sort();
        let want: Vec<U256> = landed.iter().map(|x| x.1).collect();
        let hs = format!("{h:#x}");
        let tx = raw(&p, "eth_getTransactionByHash", serde_json::json!([hs])).await?;
        let call = serde_json::json!({"from": tx["from"], "to": tx["to"], "gas": tx["gas"], "value": tx["value"], "data": tx["input"]});
        let cfg = serde_json::json!({"tracer": "callTracer", "tracerConfig": {"withLog": true}});
        match raw(&p, "debug_traceCall", serde_json::json!([call, format!("0x{:x}", *b - 1), cfg])).await {
            Ok(t) if t["error"].is_null() => {
                let mut ls = Vec::new();
                trace_logs(&t, &mut ls);
                let got = replay_amounts(&ls, &spokes);
                count_diff += (got.len() != want.len()) as u64;
                rows.push((*ti, got != want));
            }
            _ => failed += 1,
        }
    }
    let (n, m, n0, m0, n1, m1) = split(&rows);
    let ci = wilson(m, n);
    let v = serde_json::json!({
        "reexec_amount_mismatch_share": share(m, n),
        "wilson_low": ci.map(|c| c.0), "wilson_high": ci.map(|c| c.1),
        "replayed_n": n, "mismatch_count": m,
        "replayed_n_index_zero": n0, "mismatch_count_index_zero": m0, "mismatch_share_index_zero": share(m0, n0),
        "replayed_n_index_nonzero": n1, "mismatch_count_index_nonzero": m1, "mismatch_share_index_nonzero": share(m1, n1),
        "log_count_diff_count": count_diff,
        "failed_replay_count": failed,
        "v4_liquidation_tx_count": txs.len(),
        "spoke_count": spokes.len(),
        "first_block": start, "last_block": last, "blocks": blocks,
        "address_book_commit": commit,
        "provenance": {
            "started_at": started_at,
            "seeds": [seed],
            "command": format!("v4reexec --chainstack-network {network} --start {start} --blocks {blocks} --address-book-commit {commit} --seed {seed} --out {}", out.display()),
            "hardware": format!("{} on {} logical cpus", std::env::consts::ARCH, std::thread::available_parallelism().map(|x| x.get()).unwrap_or(1)),
            "wall_s": started.elapsed().as_secs_f64(),
            "data_sources": [
                {"url": format!("chainstack:{network} blocks {start} to {last} (logs, debug_traceCall at parent block)"), "license": "public chain data via the owner's Chainstack node"},
                {"url": format!("https://raw.githubusercontent.com/bgd-labs/aave-address-book/{commit}/src/AaveV4Ethereum.sol"), "license": "MIT (bgd-labs aave-address-book)"}
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
    fn split_counts_by_index() {
        assert_eq!(split(&[(0, true), (0, false), (3, true), (5, true)]), (4, 3, 2, 1, 2, 2));
        assert_eq!(split(&[]), (0, 0, 0, 0, 0, 0));
    }

    #[test]
    fn ignores_foreign_logs() {
        let sp: Vec<Address> = vec![Address::repeat_byte(1)];
        let l = serde_json::json!({"address": format!("{:#x}", Address::repeat_byte(2)), "topics": [], "data": "0x"});
        assert!(replay_amounts(&[l], &sp).is_empty());
    }
}
