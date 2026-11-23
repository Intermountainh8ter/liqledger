//! `v4updategap`: among distinct Aave V4 liquidation transactions over a pinned range that land in a block with a Chainlink
//! AnswerUpdated log at a lower transaction index, the share landing within k positions after the last such log.
//! Parameters are fixed in preregistration_additions.md. Reads only; the endpoint is never printed.

use crate::defcheck::chrono_now;
use crate::mainoracleimmediate::{chainstack_endpoint, logs_retry};
use crate::mainoracleimmediatesplit::wilson;
use crate::v4census::{fetch_source, parse_spokes, LiquidationCall};
use crate::AnswerUpdated;
use alloy::primitives::Address;
use alloy::providers::ProviderBuilder;
use alloy::rpc::types::Filter;
use alloy::sol_types::SolEvent;
use std::collections::BTreeMap;
use std::path::PathBuf;

/// (liquidations with a preceding update, within k after it, with no preceding update) from (liquidation tx index, update tx indices).
pub fn gaps(rows: &[(u64, Vec<u64>)], k: u64) -> (u64, u64, u64) {
    let (mut n, mut w, mut none) = (0, 0, 0);
    for (li, ups) in rows {
        match ups.iter().filter(|u| **u < *li).max() {
            Some(u) => {
                n += 1;
                w += (li - u <= k) as u64;
            }
            None => none += 1,
        }
    }
    (n, w, none)
}

pub async fn v4updategap(network: String, start: u64, blocks: u64, commit: String, k: u64, seed: u64, out: PathBuf) -> anyhow::Result<()> {
    let started = std::time::Instant::now();
    let started_at = chrono_now();
    anyhow::ensure!(blocks >= 1, "blocks must be at least 1");
    let last = start + blocks - 1;
    let found = parse_spokes(&fetch_source(&commit)?);
    let spokes: Vec<Address> = found.iter().filter_map(|(_, a)| a.parse().ok()).collect();
    anyhow::ensure!(!spokes.is_empty(), "no Spokes found at the pinned commit");
    let url = chainstack_endpoint(&network)?;
    let p = ProviderBuilder::new().connect_http(url.parse().map_err(|_| anyhow::anyhow!("bad {network} endpoint"))?);
    // distinct tx hash -> (block, first liquidation log tx index)
    let mut txs: BTreeMap<alloy::primitives::B256, (u64, u64)> = BTreeMap::new();
    let mut a = start;
    while a <= last {
        let z = (a + 9_999).min(last);
        let f = Filter::new().address(spokes.clone()).event_signature(LiquidationCall::SIGNATURE_HASH).from_block(a).to_block(z);
        for l in logs_retry(&p, &f).await? {
            if let (Some(h), Some(b), Some(i)) = (l.transaction_hash, l.block_number, l.transaction_index) {
                let e = txs.entry(h).or_insert((b, i));
                e.1 = e.1.min(i);
            } else {
                anyhow::bail!("ethereum eth_getLogs returned a log without a transaction index");
            }
        }
        a = z + 1;
    }
    let mut upd: BTreeMap<u64, Vec<u64>> = BTreeMap::new();
    for (b, _) in txs.values() {
        if upd.contains_key(b) {
            continue;
        }
        let f = Filter::new().event_signature(AnswerUpdated::SIGNATURE_HASH).from_block(*b).to_block(*b);
        let mut v = Vec::new();
        for l in logs_retry(&p, &f).await? {
            v.push(l.transaction_index.ok_or_else(|| anyhow::anyhow!("ethereum eth_getLogs returned a log without a transaction index"))?);
        }
        upd.insert(*b, v);
    }
    let rows: Vec<(u64, Vec<u64>)> = txs.values().map(|(b, i)| (*i, upd[b].clone())).collect();
    let (n, w, none) = gaps(&rows, k);
    let ci = wilson(w, n);
    let v = serde_json::json!({
        "liquidation_within_k_after_update_share": (n > 0).then(|| w as f64 / n as f64),
        "wilson_low": ci.map(|c| c.0), "wilson_high": ci.map(|c| c.1),
        "k": k,
        "liquidations_with_preceding_update": n,
        "liquidations_within_k_after_update": w,
        "liquidations_without_preceding_update": none,
        "v4_liquidation_tx_count": rows.len(),
        "spoke_count": spokes.len(),
        "first_block": start, "last_block": last, "blocks": blocks,
        "address_book_commit": commit,
        "provenance": {
            "started_at": started_at,
            "seeds": [seed],
            "command": format!("v4updategap --chainstack-network {network} --start {start} --blocks {blocks} --address-book-commit {commit} --k {k} --seed {seed} --out {}", out.display()),
            "hardware": format!("{} on {} logical cpus", std::env::consts::ARCH, std::thread::available_parallelism().map(|x| x.get()).unwrap_or(1)),
            "wall_s": started.elapsed().as_secs_f64(),
            "data_sources": [
                {"url": format!("chainstack:{network} blocks {start} to {last} (logs)"), "license": "public chain data via the owner's Chainstack node"},
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
    fn uses_last_preceding_update() {
        // liq at 10: updates 2 and 8 -> distance 2; liq at 5: update 9 is later -> none; liq at 20: update 3 -> distance 17
        let r = gaps(&[(10, vec![2, 8]), (5, vec![9]), (20, vec![3])], 3);
        assert_eq!(r, (2, 1, 1));
    }

    #[test]
    fn boundary_is_inclusive_and_same_index_excluded() {
        assert_eq!(gaps(&[(7, vec![4])], 3), (1, 1, 0));
        assert_eq!(gaps(&[(7, vec![3])], 3), (1, 0, 0));
        assert_eq!(gaps(&[(7, vec![7])], 3), (0, 0, 1));
    }
}
