//! `v4payment`: among distinct Aave V4 liquidation transactions over a pinned range, the share paying exactly zero
//! priority fee (effectiveGasPrice equal to the block base fee) and the visible priority fee distribution in ETH.
//! Parameters are fixed in preregistration_additions.md. Reads only; the endpoint is never printed.

use crate::defcheck::chrono_now;
use crate::mainoracleimmediate::{chainstack_endpoint, logs_retry};
use crate::mainoracleimmediatesplit::wilson;
use crate::v4census::{fetch_source, parse_spokes, LiquidationCall};
use alloy::primitives::Address;
use alloy::providers::{Provider, ProviderBuilder};
use alloy::rpc::types::Filter;
use alloy::sol_types::SolEvent;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

fn hex_u128(v: &serde_json::Value) -> Option<u128> {
    u128::from_str_radix(v.as_str()?.trim_start_matches("0x"), 16).ok()
}

/// (zero-priority count, priority fees in wei per tx) from (gas used, effective gas price, block base fee) rows.
pub fn fees(rows: &[(u128, u128, u128)]) -> (u64, Vec<u128>) {
    let mut zero = 0;
    let mut v = Vec::new();
    for (gas, egp, base) in rows {
        let tip = egp.saturating_sub(*base);
        if tip == 0 {
            zero += 1;
        }
        v.push(tip * gas);
    }
    v.sort_unstable();
    (zero, v)
}

fn quantile(v: &[u128], q: f64) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    let i = ((v.len() - 1) as f64 * q).round() as usize;
    Some(v[i] as f64 / 1e18)
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

pub async fn v4payment(network: String, start: u64, blocks: u64, commit: String, out: PathBuf) -> anyhow::Result<()> {
    let started = std::time::Instant::now();
    let started_at = chrono_now();
    anyhow::ensure!(blocks >= 1, "blocks must be at least 1");
    let last = start + blocks - 1;
    let found = parse_spokes(&fetch_source(&commit)?);
    let spokes: Vec<Address> = found.iter().filter_map(|(_, a)| a.parse().ok()).collect();
    anyhow::ensure!(!spokes.is_empty(), "no Spokes found at the pinned commit");
    let url = chainstack_endpoint(&network)?;
    let p = ProviderBuilder::new().connect_http(url.parse().map_err(|_| anyhow::anyhow!("bad {network} endpoint"))?);
    let mut txs: BTreeMap<alloy::primitives::B256, u64> = BTreeMap::new();
    let mut events = 0u64;
    let mut a = start;
    while a <= last {
        let z = (a + 9_999).min(last);
        let f = Filter::new().address(spokes.clone()).event_signature(LiquidationCall::SIGNATURE_HASH).from_block(a).to_block(z);
        for l in logs_retry(&p, &f).await? {
            events += 1;
            if let (Some(h), Some(b)) = (l.transaction_hash, l.block_number) {
                txs.insert(h, b);
            }
        }
        a = z + 1;
    }
    let mut base: BTreeMap<u64, u128> = BTreeMap::new();
    for b in txs.values().collect::<BTreeSet<_>>() {
        let h = raw(&p, "eth_getBlockByNumber", serde_json::json!([format!("0x{b:x}"), false])).await?;
        base.insert(*b, hex_u128(&h["baseFeePerGas"]).ok_or_else(|| anyhow::anyhow!("no baseFeePerGas"))?);
    }
    let mut rows = Vec::new();
    for (h, b) in &txs {
        let r = raw(&p, "eth_getTransactionReceipt", serde_json::json!([format!("{h:#x}")])).await?;
        let egp = hex_u128(&r["effectiveGasPrice"]).ok_or_else(|| anyhow::anyhow!("no effectiveGasPrice"))?;
        let gas = hex_u128(&r["gasUsed"]).ok_or_else(|| anyhow::anyhow!("no gasUsed"))?;
        rows.push((gas, egp, base[b]));
    }
    let (zero, tips) = fees(&rows);
    let n = rows.len() as u64;
    let share = if n > 0 { serde_json::json!(zero as f64 / n as f64) } else { serde_json::Value::Null };
    let ci = wilson(zero, n);
    let v = serde_json::json!({
        "v4_zero_priority_fee_share": share,
        "wilson_low": ci.map(|c| c.0), "wilson_high": ci.map(|c| c.1),
        "v4_liquidation_event_count": events,
        "v4_liquidation_tx_count": n,
        "zero_priority_txs": zero,
        "priority_fee_eth_median": quantile(&tips, 0.5),
        "priority_fee_eth_p90": quantile(&tips, 0.9),
        "priority_fee_eth_max": quantile(&tips, 1.0),
        "spoke_count": spokes.len(),
        "first_block": start, "last_block": last, "blocks": blocks,
        "address_book_commit": commit,
        "provenance": {
            "started_at": started_at,
            "seeds": [7],
            "command": format!("v4payment --chainstack-network {network} --start {start} --blocks {blocks} --address-book-commit {commit} --out {}", out.display()),
            "hardware": format!("{} on {} logical cpus", std::env::consts::ARCH, std::thread::available_parallelism().map(|x| x.get()).unwrap_or(1)),
            "wall_s": started.elapsed().as_secs_f64(),
            "data_sources": [
                {"url": format!("chainstack:{network} blocks {start} to {last} (logs, receipts, block headers)"), "license": "public chain data via the owner's Chainstack node"},
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
    fn counts_zero_tips() {
        let (z, t) = fees(&[(100, 10, 10), (100, 12, 10), (50, 9, 10)]);
        assert_eq!(z, 2);
        assert_eq!(t, vec![0, 0, 200]);
    }

    #[test]
    fn quantile_empty_and_max() {
        assert_eq!(quantile(&[], 0.5), None);
        assert_eq!(quantile(&[0, 2_000_000_000_000_000_000], 1.0), Some(2.0));
    }
}
