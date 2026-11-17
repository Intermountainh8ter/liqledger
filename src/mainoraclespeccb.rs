//! `mainoraclespeccb`: among Aave V3 liquidation transactions, the share with a nonzero traced coinbase transfer for
//! speculative transactions (no AnswerUpdated in an earlier transaction of the same block or the 2 preceding blocks)
//! versus the rest. Parameters are fixed in preregistration_additions.md. Reads only; the endpoint is never printed.

use crate::defcheck::chrono_now;
use crate::mainoracleimmediate::{chainstack_endpoint, liq_logs, logs_retry, pos};
use crate::mainoracleimmediatecbshare::coinbase_value;
use crate::AnswerUpdated;
use alloy::providers::{Provider, ProviderBuilder};
use alloy::rpc::types::Filter;
use alloy::sol_types::SolEvent;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

const MIN_TXS_ALL: usize = 100;
const MIN_CLASS: usize = 20;
const TRACE_CAP: usize = 1500;

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

/// True when no update sits in an earlier transaction of the same block or in the 2 preceding blocks.
pub fn is_speculative(update_txs: &BTreeSet<(u64, u64)>, l: (u64, u64)) -> bool {
    let earlier = update_txs.iter().any(|u| u.0 == l.0 && u.1 < l.1);
    let prior = update_txs.iter().any(|u| u.0 < l.0 && u.0 + 2 >= l.0);
    !earlier && !prior
}

fn share(flags: &[bool]) -> Option<f64> {
    if flags.is_empty() { None } else { Some(flags.iter().filter(|f| **f).count() as f64 / flags.len() as f64) }
}

fn summarize(spec: &[bool], non: &[bool], spec_total: usize, non_total: usize) -> serde_json::Value {
    let abstain = spec.len() + non.len() < MIN_TXS_ALL || spec.len() < MIN_CLASS || non.len() < MIN_CLASS;
    let (s, n) = (share(spec), share(non));
    let diff = match (s, n) { (Some(s), Some(n)) if !abstain => Some(s - n), _ => None };
    serde_json::json!({
        "spec_txs": spec_total, "nonspec_txs": non_total, "spec_traced": spec.len(), "nonspec_traced": non.len(),
        "spec_cb_share": s, "nonspec_cb_share": n, "spec_minus_nonspec_cb_share": diff, "abstain": abstain,
    })
}

pub async fn mainoraclespeccb(network: String, start: u64, blocks: u64, out: PathBuf) -> anyhow::Result<()> {
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
    let liq_blocks: BTreeSet<u64> = txs.keys().map(|k| k.0).collect();
    let need: BTreeSet<u64> = liq_blocks.iter().flat_map(|b| [b.saturating_sub(2), b.saturating_sub(1), *b]).collect();
    let mut update_txs = BTreeSet::new();
    for b in &need {
        let f = Filter::new().event_signature(AnswerUpdated::SIGNATURE_HASH).from_block(*b).to_block(*b);
        update_txs.extend(pos(&logs_retry(&p, &f).await?));
    }
    let mut heads: BTreeMap<u64, String> = BTreeMap::new();
    let mut groups: [Vec<(u64, u64)>; 2] = [Vec::new(), Vec::new()];
    for (b, i) in txs.keys() {
        if !heads.contains_key(b) {
            let hd = raw(&p, "eth_getBlockByNumber", serde_json::json!([format!("0x{b:x}"), false])).await?;
            heads.insert(*b, hd["miner"].as_str().ok_or_else(|| anyhow::anyhow!("no miner"))?.to_lowercase());
        }
        groups[if is_speculative(&update_txs, (*b, *i)) { 0 } else { 1 }].push((*b, *i));
    }
    let totals = [groups[0].len(), groups[1].len()];
    let mut flags: [Vec<bool>; 2] = [Vec::new(), Vec::new()];
    for g in 0..2 {
        for (b, i) in groups[g].iter().take(TRACE_CAP) {
            let tr = raw(&p, "trace_transaction", serde_json::json!([format!("{:#x}", txs[&(*b, *i)])])).await?;
            flags[g].push(coinbase_value(&tr, &heads[b]) > 0);
        }
    }
    let mut range = summarize(&flags[0], &flags[1], totals[0], totals[1]);
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
            "command": format!("mainoraclespeccb --chainstack-network {network} --start {start} --blocks {blocks} --out {}", out.display()),
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
    fn classifies_speculative() {
        let u: BTreeSet<_> = [(10u64, 3u64), (20, 4)].into();
        assert!(!is_speculative(&u, (10, 9)));
        assert!(!is_speculative(&u, (12, 1)));
        assert!(is_speculative(&u, (13, 1)));
        assert!(is_speculative(&u, (20, 4)));
        assert!(is_speculative(&u, (10, 2)));
    }

    #[test]
    fn summarize_diff_and_abstain() {
        let s = summarize(&vec![true; 60], &vec![false; 60], 60, 60);
        assert_eq!(s["spec_minus_nonspec_cb_share"].as_f64().unwrap(), 1.0);
        let s = summarize(&vec![true; 90], &vec![false; 10], 90, 10);
        assert_eq!(s["abstain"], true);
        assert!(s["spec_minus_nonspec_cb_share"].is_null());
    }
}
