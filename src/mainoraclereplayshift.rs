//! `mainoraclereplayshift`: replay each landed Aave V3 liquidation transaction with eth_call (same from, to, input,
//! value) at the parent block state and at the state one block earlier, and count transactions whose success flag
//! differs. Parameters are fixed in preregistration_additions.md. Reads only; the endpoint is never printed.

use crate::defcheck::chrono_now;
use crate::mainoracleimmediate::{chainstack_endpoint, liq_logs};
use alloy::providers::{Provider, ProviderBuilder};
use alloy::transports::RpcError;
use std::collections::BTreeMap;
use std::path::PathBuf;

const MIN_TXS: u64 = 100;

/// (parent_ok, earlier_ok) per replayed transaction.
fn summarize(rows: &[(bool, bool)]) -> serde_json::Value {
    let n = rows.len() as u64;
    let differs = rows.iter().filter(|r| r.0 != r.1).count() as u64;
    let ok_to_fail = rows.iter().filter(|r| r.0 && !r.1).count() as u64;
    let parent_ok = rows.iter().filter(|r| r.0).count() as u64;
    let earlier_ok = rows.iter().filter(|r| r.1).count() as u64;
    serde_json::json!({
        "replayed_txs": n,
        "parent_state_success": parent_ok,
        "earlier_state_success": earlier_ok,
        "parent_ok_earlier_fail": ok_to_fail,
        "state_shift_differs_count": differs,
        "state_shift_differs_share": if n > 0 { serde_json::json!(differs as f64 / n as f64) } else { serde_json::Value::Null },
        "abstain": n < MIN_TXS,
    })
}

/// Ok(true) when the call returns, Ok(false) when the node reports an execution error (revert); transport errors retry.
async fn call_ok(p: &impl Provider, tx: &serde_json::Value, block: u64) -> anyhow::Result<bool> {
    let mut tries = 0;
    loop {
        match p
            .raw_request::<_, serde_json::Value>("eth_call".into(), serde_json::json!([tx, format!("0x{block:x}")]))
            .await
        {
            Ok(_) => return Ok(true),
            Err(RpcError::ErrorResp(e)) if e.message.to_lowercase().contains("revert") || e.code == 3 => return Ok(false),
            Err(_) if tries < 5 => {
                tries += 1;
                tokio::time::sleep(std::time::Duration::from_millis(500 * (1 << tries))).await;
            }
            Err(_) => anyhow::bail!("ethereum eth_call failed"),
        }
    }
}

async fn tx_by_hash(p: &impl Provider, h: &str) -> anyhow::Result<serde_json::Value> {
    let mut tries = 0;
    loop {
        match p.raw_request::<_, serde_json::Value>("eth_getTransactionByHash".into(), serde_json::json!([h])).await {
            Ok(v) if !v.is_null() => return Ok(v),
            _ if tries < 5 => {
                tries += 1;
                tokio::time::sleep(std::time::Duration::from_millis(500 * (1 << tries))).await;
            }
            _ => anyhow::bail!("ethereum eth_getTransactionByHash failed"),
        }
    }
}

pub async fn mainoraclereplayshift(network: String, start: u64, blocks: u64, out: PathBuf) -> anyhow::Result<()> {
    let started = std::time::Instant::now();
    let started_at = chrono_now();
    anyhow::ensure!(blocks >= 1 && start >= 2, "blocks must be at least 1 and start at least 2");
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
    let mut rows = Vec::new();
    for ((b, _), h) in &txs {
        let t = tx_by_hash(&p, &format!("{h:#x}")).await?;
        let call = serde_json::json!({"from": t["from"], "to": t["to"], "data": t["input"], "value": t["value"]});
        let parent = call_ok(&p, &call, b - 1).await?;
        let earlier = call_ok(&p, &call, b - 2).await?;
        rows.push((parent, earlier));
    }
    let mut r = summarize(&rows);
    let o = r.as_object_mut().unwrap();
    o.insert("first_block".into(), start.into());
    o.insert("last_block".into(), last.into());
    o.insert("blocks".into(), blocks.into());
    o.insert("liquidation_logs".into(), logs.len().into());
    let v = serde_json::json!({
        "ranges": [r],
        "provenance": {
            "started_at": started_at,
            "seeds": [7],
            "command": format!("mainoraclereplayshift --chainstack-network {network} --start {start} --blocks {blocks} --out {}", out.display()),
            "hardware": format!("{} on {} logical cpus", std::env::consts::ARCH, std::thread::available_parallelism().map(|x| x.get()).unwrap_or(1)),
            "wall_s": started.elapsed().as_secs_f64(),
            "data_sources": [
                {"url": format!("chainstack:ethereum blocks {start} to {last} (logs, transactions, eth_call at fixed historical blocks)"),
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
    fn counts_differences_only() {
        let mut rows = vec![(true, false), (false, true), (true, true), (false, false)];
        rows.extend(std::iter::repeat((true, true)).take(100));
        let s = summarize(&rows);
        assert_eq!(s["state_shift_differs_count"], 2);
        assert_eq!(s["parent_ok_earlier_fail"], 1);
        assert_eq!(s["abstain"], false);
    }

    #[test]
    fn abstains_below_minimum() {
        let s = summarize(&[(true, false)]);
        assert_eq!(s["abstain"], true);
        assert_eq!(s["state_shift_differs_count"], 1);
    }
}
