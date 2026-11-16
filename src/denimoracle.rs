//! `denimoracle`: share of Aave V3 liquidation blocks on Base that also hold a Chainlink AnswerUpdated log, and the
//! median transaction-index offset. Parameters are fixed in preregistration_additions.md. Reads only; the endpoint
//! comes from an environment variable and is never printed.

use crate::defcheck::chrono_now;
use crate::{addr, AnswerUpdated, LiquidationCall};
use alloy::providers::{Provider, ProviderBuilder};
use alloy::rpc::types::{Filter, Log};
use alloy::sol_types::SolEvent;
use std::collections::BTreeMap;
use std::path::PathBuf;

const BASE_AAVE_V3_POOL: &str = "0xA238Dd80C259a72e81d7e4664a9801593F98d1c5";
const MIN_BLOCKS: u64 = 20;

/// Per liquidation block: (liquidation tx index, first update tx index if any). Uses the first liquidation log.
pub fn block_offsets(liqs: &[(u64, u64)], updates: &[(u64, u64)]) -> BTreeMap<u64, (u64, Option<u64>)> {
    let mut first_liq: BTreeMap<u64, u64> = BTreeMap::new();
    for &(b, i) in liqs {
        first_liq.entry(b).and_modify(|x| *x = (*x).min(i)).or_insert(i);
    }
    let mut first_upd: BTreeMap<u64, u64> = BTreeMap::new();
    for &(b, i) in updates {
        first_upd.entry(b).and_modify(|x| *x = (*x).min(i)).or_insert(i);
    }
    first_liq.into_iter().map(|(b, i)| (b, (i, first_upd.get(&b).copied()))).collect()
}

pub fn median_i64(v: &mut [i64]) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    v.sort_unstable();
    let n = v.len();
    Some(if n % 2 == 1 { v[n / 2] as f64 } else { (v[n / 2 - 1] + v[n / 2]) as f64 / 2.0 })
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
            Err(_) => anyhow::bail!("base eth_getLogs failed"),
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

pub async fn denimoracle(rpc_env: String, start: u64, blocks: u64, out: PathBuf) -> anyhow::Result<()> {
    let started = std::time::Instant::now();
    let started_at = chrono_now();
    anyhow::ensure!(blocks >= 1, "blocks must be at least 1");
    let url = std::env::var(&rpc_env).map_err(|_| anyhow::anyhow!("env {rpc_env} not set"))?;
    let p = ProviderBuilder::new().connect_http(url.parse().map_err(|_| anyhow::anyhow!("bad endpoint in {rpc_env}"))?);
    let last = start + blocks - 1;
    let mut liqs = Vec::new();
    let mut a = start;
    while a <= last {
        let z = (a + 9_999).min(last);
        liqs.extend(pos(&liq_logs(&p, a, z).await?));
        a = z + 1;
    }
    let liq_blocks: Vec<u64> = block_offsets(&liqs, &[]).keys().copied().collect();
    let mut updates = Vec::new();
    for b in &liq_blocks {
        let f = Filter::new().event_signature(AnswerUpdated::SIGNATURE_HASH).from_block(*b).to_block(*b);
        updates.extend(pos(&logs_retry(&p, &f).await?));
    }
    let m = block_offsets(&liqs, &updates);
    let n = m.len() as u64;
    let with = m.values().filter(|(_, u)| u.is_some()).count() as u64;
    let mut offs: Vec<i64> = m.values().filter_map(|(l, u)| u.map(|u| *l as i64 - u as i64)).collect();
    let share = if n > 0 { serde_json::json!(with as f64 / n as f64) } else { serde_json::Value::Null };
    let v = serde_json::json!({
        "ranges": [{
            "first_block": start, "last_block": last, "blocks": blocks,
            "liquidation_logs": liqs.len(), "liquidation_blocks": n,
            "liquidation_blocks_with_update": with,
            "same_block_update_share": share,
            "median_tx_index_offset": median_i64(&mut offs),
            "abstain": n < MIN_BLOCKS,
        }],
        "provenance": {
            "started_at": started_at,
            "seeds": [7],
            "command": format!("denimoracle --rpc-env {rpc_env} --start {start} --blocks {blocks} --out results/179-denimoracle.json"),
            "hardware": format!("{} on {} logical cpus", std::env::consts::ARCH, std::thread::available_parallelism().map(|x| x.get()).unwrap_or(1)),
            "wall_s": started.elapsed().as_secs_f64(),
            "data_sources": [
                {"url": format!("chainstack:base blocks {start} to {last} (logs)"),
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
    fn offsets_pair_first_logs() {
        let m = block_offsets(&[(10, 5), (10, 3), (11, 7)], &[(10, 1), (10, 2), (12, 0)]);
        assert_eq!(m[&10], (3, Some(1)));
        assert_eq!(m[&11], (7, None));
        assert_eq!(m.len(), 2);
    }

    #[test]
    fn median_works() {
        assert_eq!(median_i64(&mut []), None);
        assert_eq!(median_i64(&mut [3, -1, 2]), Some(2.0));
        assert_eq!(median_i64(&mut [1, 2, 3, 4]), Some(2.5));
    }
}
