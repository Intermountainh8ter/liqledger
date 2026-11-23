//! `v4bonuschg`: for every V4 liquidation transaction, reads the collateral reserve's maxLiquidationBonus (uint32-keyed dynamic config
//! from the borrower's own position) at block N-1 and at the landing block N, and counts transactions where the two differ. Reads only; the endpoint is never printed.

use crate::defcheck::chrono_now;
use crate::mainoracleimmediate::{chainstack_endpoint, logs_retry};
use crate::v4bonuscfg32::IV4Cfg32;
use crate::v4bonuscmp::max_bonus_bps;
use crate::v4census::{fetch_source, parse_spokes, LiquidationCall};
use alloy::eips::BlockId;
use alloy::primitives::{Address, B256, U256};
use alloy::providers::ProviderBuilder;
use alloy::rpc::types::Filter;
use alloy::sol_types::SolEvent;
use std::collections::BTreeMap;
use std::path::PathBuf;

type Ev = (u64, Address, Address, U256);

async fn max_at<P: alloy::providers::Provider>(p: &P, ev: &Ev, rb: u64) -> Option<i128> {
    let (_, spoke, user, crid) = *ev;
    let t = std::time::Duration::from_secs(30);
    let blk = BlockId::number(rb);
    let sp = IV4Cfg32::new(spoke, p);
    let pos = tokio::time::timeout(t, sp.getUserPosition(crid, user).block(blk).call()).await.ok()?.ok()?;
    let cfg = tokio::time::timeout(t, sp.getDynamicReserveConfig(crid, pos.dynamicConfigKey).block(blk).call()).await.ok()?.ok()?;
    max_bonus_bps(cfg.maxLiquidationBonus)
}

/// True when both reads exist and differ.
pub fn changed(before: i128, after: i128) -> bool {
    before != after
}

pub async fn v4bonuschg(network: String, start: u64, blocks: u64, commit: String, seed: u64, out: PathBuf) -> anyhow::Result<()> {
    let started = std::time::Instant::now();
    let started_at = chrono_now();
    anyhow::ensure!(blocks >= 1 && start >= 2, "blocks must be at least 1 and start at least 2");
    let last = start + blocks - 1;
    let found = parse_spokes(&fetch_source(&commit)?);
    let spokes: Vec<Address> = found.iter().filter_map(|(_, a)| a.parse().ok()).collect();
    anyhow::ensure!(!spokes.is_empty(), "no Spokes found at the pinned commit");
    let url = chainstack_endpoint(&network)?;
    let p = ProviderBuilder::new().connect_http(url.parse().map_err(|_| anyhow::anyhow!("bad {network} endpoint"))?);
    let mut txs: BTreeMap<B256, Vec<Ev>> = BTreeMap::new();
    let mut a = start;
    while a <= last {
        let z = (a + 9_999).min(last);
        let f = Filter::new().address(spokes.clone()).event_signature(LiquidationCall::SIGNATURE_HASH).from_block(a).to_block(z);
        for l in logs_retry(&p, &f).await? {
            let (Some(h), Some(b)) = (l.transaction_hash, l.block_number) else {
                anyhow::bail!("ethereum eth_getLogs returned a log without position fields");
            };
            let ev = LiquidationCall::decode_log(&l.inner).map_err(|_| anyhow::anyhow!("ethereum eth_getLogs returned an undecodable LiquidationCall"))?;
            txs.entry(h).or_default().push((b, l.address(), ev.data.user, ev.data.collateralReserveId));
        }
        a = z + 1;
    }
    let (mut changed_n, mut unreadable) = (0u64, 0u64);
    let mut details: Vec<serde_json::Value> = Vec::new();
    for evs in txs.values() {
        let (mut diff, mut fail) = (false, false);
        let mut pair: Option<(i128, i128)> = None;
        for ev in evs {
            match (max_at(&p, ev, ev.0 - 1).await, max_at(&p, ev, ev.0).await) {
                (Some(x), Some(y)) => {
                    if changed(x, y) {
                        diff = true;
                        pair = Some((x, y));
                    }
                }
                _ => fail = true,
            }
        }
        if fail {
            unreadable += 1;
        } else if diff {
            changed_n += 1;
            let (x, y) = pair.unwrap_or((0, 0));
            details.push(serde_json::json!({"block": evs[0].0, "spoke": format!("{:?}", evs[0].1), "max_bonus_bps_n_minus_1": x, "max_bonus_bps_n": y}));
        }
    }
    let v = serde_json::json!({
        "config_changed_between_n_minus_1_and_landing_tx_count": changed_n,
        "unreadable_tx_count": unreadable,
        "changed_details": details,
        "v4_liquidation_tx_count": txs.len(),
        "spoke_count": spokes.len(),
        "first_block": start, "last_block": last, "blocks": blocks,
        "address_book_commit": commit,
        "provenance": {
            "started_at": started_at,
            "seeds": [seed],
            "command": format!("v4bonuschg --chainstack-network {network} --start {start} --blocks {blocks} --address-book-commit {commit} --seed {seed} --out {}", out.display()),
            "hardware": format!("{} on {} logical cpus", std::env::consts::ARCH, std::thread::available_parallelism().map(|x| x.get()).unwrap_or(1)),
            "wall_s": started.elapsed().as_secs_f64(),
            "data_sources": [
                {"url": format!("chainstack:{network} blocks {start} to {last} (logs, eth_call at blocks N-1 and N)"), "license": "public chain data via the owner's Chainstack node"},
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
    fn change_detects_difference_only() {
        assert!(changed(500, 600));
        assert!(!changed(500, 500));
    }
}
