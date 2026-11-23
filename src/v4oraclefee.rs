//! `v4oraclefee`: among distinct Aave V4 liquidation transactions over a pinned range, the share landing in a block with a
//! Chainlink AnswerUpdated log (any feed), split by exactly zero versus positive priority fee. Parameters are fixed in
//! preregistration_additions.md. Reads only; the endpoint is never printed.

use crate::defcheck::chrono_now;
use crate::mainoracleimmediate::{chainstack_endpoint, logs_retry};
use crate::mainoracleimmediatesplit::wilson;
use crate::v4census::{fetch_source, parse_spokes, LiquidationCall};
use crate::AnswerUpdated;
use alloy::primitives::Address;
use alloy::providers::{Provider, ProviderBuilder};
use alloy::rpc::types::Filter;
use alloy::sol_types::SolEvent;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

fn hex_u128(v: &serde_json::Value) -> Option<u128> {
    u128::from_str_radix(v.as_str()?.trim_start_matches("0x"), 16).ok()
}

/// (zero-fee count, zero-fee with update, paid count, paid with update) from (tip is zero, block has update) rows.
pub fn split(rows: &[(bool, bool)]) -> (u64, u64, u64, u64) {
    let mut r = (0, 0, 0, 0);
    for (zero, upd) in rows {
        if *zero {
            r.0 += 1;
            r.1 += *upd as u64;
        } else {
            r.2 += 1;
            r.3 += *upd as u64;
        }
    }
    r
}

fn share(k: u64, n: u64) -> Option<f64> {
    (n > 0).then(|| k as f64 / n as f64)
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

pub async fn v4oraclefee(network: String, start: u64, blocks: u64, commit: String, out: PathBuf) -> anyhow::Result<()> {
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
    let mut a = start;
    while a <= last {
        let z = (a + 9_999).min(last);
        let f = Filter::new().address(spokes.clone()).event_signature(LiquidationCall::SIGNATURE_HASH).from_block(a).to_block(z);
        for l in logs_retry(&p, &f).await? {
            if let (Some(h), Some(b)) = (l.transaction_hash, l.block_number) {
                txs.insert(h, b);
            }
        }
        a = z + 1;
    }
    let mut base: BTreeMap<u64, u128> = BTreeMap::new();
    let mut upd: BTreeMap<u64, bool> = BTreeMap::new();
    for b in txs.values().collect::<BTreeSet<_>>() {
        let h = raw(&p, "eth_getBlockByNumber", serde_json::json!([format!("0x{b:x}"), false])).await?;
        base.insert(*b, hex_u128(&h["baseFeePerGas"]).ok_or_else(|| anyhow::anyhow!("no baseFeePerGas"))?);
        let f = Filter::new().event_signature(AnswerUpdated::SIGNATURE_HASH).from_block(*b).to_block(*b);
        upd.insert(*b, !logs_retry(&p, &f).await?.is_empty());
    }
    let mut rows = Vec::new();
    for (h, b) in &txs {
        let r = raw(&p, "eth_getTransactionReceipt", serde_json::json!([format!("{h:#x}")])).await?;
        let egp = hex_u128(&r["effectiveGasPrice"]).ok_or_else(|| anyhow::anyhow!("no effectiveGasPrice"))?;
        rows.push((egp.saturating_sub(base[b]) == 0, upd[b]));
    }
    let (zn, zw, pn, pw) = split(&rows);
    let zci = wilson(zw, zn);
    let pci = wilson(pw, pn);
    let v = serde_json::json!({
        "oracle_adjacent_share_zero_fee": share(zw, zn),
        "zero_fee_wilson_low": zci.map(|c| c.0), "zero_fee_wilson_high": zci.map(|c| c.1),
        "oracle_adjacent_share_paid": share(pw, pn),
        "paid_wilson_low": pci.map(|c| c.0), "paid_wilson_high": pci.map(|c| c.1),
        "zero_fee_tx_count": zn, "zero_fee_tx_with_update": zw,
        "paid_tx_count": pn, "paid_tx_with_update": pw,
        "v4_liquidation_tx_count": rows.len(),
        "spoke_count": spokes.len(),
        "first_block": start, "last_block": last, "blocks": blocks,
        "address_book_commit": commit,
        "provenance": {
            "started_at": started_at,
            "seeds": [7],
            "command": format!("v4oraclefee --chainstack-network {network} --start {start} --blocks {blocks} --address-book-commit {commit} --out {}", out.display()),
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
    fn splits_groups() {
        let r = split(&[(true, true), (true, false), (false, true), (false, true), (false, false)]);
        assert_eq!(r, (2, 1, 3, 2));
    }

    #[test]
    fn share_empty_is_none() {
        assert_eq!(share(0, 0), None);
        assert_eq!(share(1, 4), Some(0.25));
    }
}
