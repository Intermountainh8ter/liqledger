//! `v4bonuscmp`: for the V4 liquidation transactions whose Spoke bonus call reverts (debt id retry too), read maxLiquidationBonus from the
//! uint32-keyed dynamic config at block N-1, compute the landed bonus from collateral and debt values removed, and count transactions whose
//! landed bonus exceeds the configured maximum by more than a tolerance. Parameters are fixed in preregistration_additions.md. Reads only; the endpoint is never printed.

use crate::defcheck::chrono_now;
use crate::mainoracleimmediate::{chainstack_endpoint, logs_retry};
use crate::v4bonuscfg32::IV4Cfg32;
use crate::v4bonuscurve::{landed_bonus_bps, IV4Bonus};
use crate::v4bonusdiag::{classify, Outcome};
use crate::v4census::{fetch_source, parse_spokes, LiquidationCall};
use crate::v4dustrule::{debt_value, IV4Oracle, IV4Spoke};
use alloy::eips::BlockId;
use alloy::primitives::{Address, B256, U256};
use alloy::providers::ProviderBuilder;
use alloy::rpc::types::Filter;
use alloy::sol_types::SolEvent;
use std::collections::BTreeMap;
use std::path::PathBuf;

type Ev = (u64, Address, Address, U256, U256, U256, U256);

/// Configured maximum bonus in basis points over par from the raw value (10000 is par); None when below par or above 3x par.
pub fn max_bonus_bps(raw: u32) -> Option<i128> {
    if !(10_000..=30_000).contains(&raw) {
        return None;
    }
    Some(raw as i128 - 10_000)
}

/// True when the landed bonus exceeds the configured maximum by more than the tolerance.
pub fn exceeds(landed: i128, max: i128, tol_bps: i128) -> bool {
    landed - max > tol_bps
}

async fn bonus_reverts<P: alloy::providers::Provider>(p: &P, ev: &Ev, rid: U256) -> bool {
    let (b, spoke, user, ..) = *ev;
    let t = std::time::Duration::from_secs(30);
    let blk = BlockId::number(b - 1);
    let sp = IV4Bonus::new(spoke, p);
    let Some(hf) = tokio::time::timeout(t, sp.getUserAccountData(user).block(blk).call()).await.ok().and_then(|r| r.ok()).map(|d| d.healthFactor) else {
        return false;
    };
    match tokio::time::timeout(t, sp.getLiquidationBonus(rid, user, hf).block(blk).call()).await {
        Ok(Err(e)) => classify(&e) != Outcome::Ok,
        _ => false,
    }
}

async fn read_val<P: alloy::providers::Provider>(p: &P, spoke: Address, rid: U256, amount: U256, b: u64) -> Option<U256> {
    let t = std::time::Duration::from_secs(30);
    let blk = BlockId::number(b);
    let sp = IV4Spoke::new(spoke, p);
    let oracle = tokio::time::timeout(t, sp.ORACLE().block(blk).call()).await.ok()?.ok()?;
    let res = tokio::time::timeout(t, sp.getReserve(rid).block(blk).call()).await.ok()?.ok()?;
    let o = IV4Oracle::new(oracle, p);
    let price = tokio::time::timeout(t, o.getReservePrice(rid).block(blk).call()).await.ok()?.ok()?;
    let od = tokio::time::timeout(t, o.DECIMALS().block(blk).call()).await.ok()?.ok().unwrap_or(8);
    debt_value(amount, res.decimals, price, od)
}

/// (landed, max) in bps for one event, or an unreadable reason.
async fn event_bonus<P: alloy::providers::Provider>(p: &P, ev: &Ev) -> Result<(i128, i128), &'static str> {
    let (b, spoke, user, crid, drid, debt_amt, coll_amt) = *ev;
    let t = std::time::Duration::from_secs(30);
    let blk = BlockId::number(b - 1);
    let sp = IV4Cfg32::new(spoke, p);
    let pos = tokio::time::timeout(t, sp.getUserPosition(crid, user).block(blk).call()).await.ok().and_then(|r| r.ok()).ok_or("position unreadable")?;
    let cfg = tokio::time::timeout(t, sp.getDynamicReserveConfig(crid, pos.dynamicConfigKey).block(blk).call())
        .await
        .ok()
        .and_then(|r| r.ok())
        .ok_or("dynamic config unreadable")?;
    let max = max_bonus_bps(cfg.maxLiquidationBonus).ok_or("max bonus out of range")?;
    let cv = read_val(p, spoke, crid, coll_amt, b - 1).await.ok_or("price or reserve unreadable")?;
    let dv = read_val(p, spoke, drid, debt_amt, b - 1).await.ok_or("price or reserve unreadable")?;
    let landed = landed_bonus_bps(cv, dv).ok_or("zero debt value")?;
    Ok((landed, max))
}

pub async fn v4bonuscmp(network: String, start: u64, blocks: u64, commit: String, tolerance_bps: u64, seed: u64, out: PathBuf) -> anyhow::Result<()> {
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
            let d = &ev.data;
            txs.entry(h).or_default().push((b, l.address(), d.user, d.collateralReserveId, d.debtReserveId, d.debtAmountRestored, d.collateralAmountRemoved));
        }
        a = z + 1;
    }
    let tol = tolerance_bps as i128;
    let (mut reverting, mut within, mut over, mut unreadable) = (0u64, 0u64, 0u64, 0u64);
    let mut reasons: BTreeMap<&'static str, u64> = BTreeMap::new();
    let mut by_spoke: BTreeMap<String, BTreeMap<&'static str, u64>> = BTreeMap::new();
    let mut max_excess = i128::MIN;
    for evs in txs.values() {
        let mut still: Vec<&Ev> = Vec::new();
        for ev in evs {
            if bonus_reverts(&p, ev, ev.3).await && bonus_reverts(&p, ev, ev.4).await {
                still.push(ev);
            }
        }
        if still.is_empty() {
            continue;
        }
        reverting += 1;
        let spoke = by_spoke.entry(format!("{:?}", still[0].1)).or_default();
        let (mut tx_over, mut fail) = (false, None);
        for ev in &still {
            match event_bonus(&p, ev).await {
                Ok((l, m)) => {
                    max_excess = max_excess.max(l - m);
                    tx_over |= exceeds(l, m, tol);
                }
                Err(r) => {
                    fail.get_or_insert(r);
                }
            }
        }
        match fail {
            Some(r) => {
                unreadable += 1;
                *reasons.entry(r).or_default() += 1;
                *spoke.entry("unreadable").or_default() += 1;
            }
            None if tx_over => {
                over += 1;
                *spoke.entry("above-max").or_default() += 1;
            }
            None => {
                within += 1;
                *spoke.entry("within-max").or_default() += 1;
            }
        }
    }
    let v = serde_json::json!({
        "landed_within_max_bonus_tx_count": within,
        "landed_above_max_bonus_tx_count": over,
        "unreadable_tx_count": unreadable,
        "unreadable_by_reason": reasons,
        "reverting_tx_count": reverting,
        "max_landed_minus_configured_bps": if max_excess == i128::MIN { serde_json::Value::Null } else { serde_json::json!(max_excess) },
        "tolerance_bps": tolerance_bps,
        "comparison_by_spoke": by_spoke,
        "v4_liquidation_tx_count": txs.len(),
        "spoke_count": spokes.len(),
        "first_block": start, "last_block": last, "blocks": blocks,
        "address_book_commit": commit,
        "provenance": {
            "started_at": started_at,
            "seeds": [seed],
            "command": format!("v4bonuscmp --chainstack-network {network} --start {start} --blocks {blocks} --address-book-commit {commit} --tolerance-bps {tolerance_bps} --seed {seed} --out {}", out.display()),
            "hardware": format!("{} on {} logical cpus", std::env::consts::ARCH, std::thread::available_parallelism().map(|x| x.get()).unwrap_or(1)),
            "wall_s": started.elapsed().as_secs_f64(),
            "data_sources": [
                {"url": format!("chainstack:{network} blocks {start} to {last} (logs, eth_call at block N-1)"), "license": "public chain data via the owner's Chainstack node"},
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
    fn max_bonus_range() {
        assert_eq!(max_bonus_bps(10_500), Some(500));
        assert_eq!(max_bonus_bps(9_999), None);
        assert_eq!(max_bonus_bps(30_001), None);
    }

    #[test]
    fn exceeds_edges() {
        assert!(!exceeds(600, 500, 100));
        assert!(exceeds(601, 500, 100));
        assert!(!exceeds(100, 500, 100));
    }
}
