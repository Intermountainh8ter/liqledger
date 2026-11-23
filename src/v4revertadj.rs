//! `v4revertadj`: splits the V4 liquidation transactions into reverting-bonus-call and other, and counts in each group those with a Chainlink
//! AnswerUpdated log at a lower transaction index in the same block. Also flags the above-max cases at N-1. Parameters are fixed in
//! preregistration_additions.md. Reads only; the endpoint is never printed.

use crate::defcheck::chrono_now;
use crate::mainoracleimmediate::{chainstack_endpoint, logs_retry};
use crate::v4bonuscfg32::IV4Cfg32;
use crate::v4bonuscurve::{landed_bonus_bps, IV4Bonus};
use crate::v4bonusdiag::{classify, Outcome};
use crate::v4bonuscmp::{exceeds, max_bonus_bps};
use crate::v4census::{fetch_source, parse_spokes, LiquidationCall};
use crate::v4dustrule::{debt_value, IV4Oracle, IV4Spoke};
use crate::AnswerUpdated;
use alloy::eips::BlockId;
use alloy::primitives::{Address, B256, U256};
use alloy::providers::ProviderBuilder;
use alloy::rpc::types::Filter;
use alloy::sol_types::SolEvent;
use std::collections::BTreeMap;
use std::path::PathBuf;

type Ev = (u64, Address, Address, U256, U256, U256, U256);

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
async fn event_bonus<P: alloy::providers::Provider>(p: &P, ev: &Ev, rb: u64) -> Result<(i128, i128), &'static str> {
    let (_, spoke, user, crid, drid, debt_amt, coll_amt) = *ev;
    let t = std::time::Duration::from_secs(30);
    let blk = BlockId::number(rb);
    let sp = IV4Cfg32::new(spoke, p);
    let pos = tokio::time::timeout(t, sp.getUserPosition(crid, user).block(blk).call()).await.ok().and_then(|r| r.ok()).ok_or("position unreadable")?;
    let cfg = tokio::time::timeout(t, sp.getDynamicReserveConfig(crid, pos.dynamicConfigKey).block(blk).call())
        .await
        .ok()
        .and_then(|r| r.ok())
        .ok_or("dynamic config unreadable")?;
    let max = max_bonus_bps(cfg.maxLiquidationBonus).ok_or("max bonus out of range")?;
    let cv = read_val(p, spoke, crid, coll_amt, rb).await.ok_or("price or reserve unreadable")?;
    let dv = read_val(p, spoke, drid, debt_amt, rb).await.ok_or("price or reserve unreadable")?;
    let landed = landed_bonus_bps(cv, dv).ok_or("zero debt value")?;
    Ok((landed, max))
}

/// True if any update log sits at a lower transaction index than the liquidation.
fn preceded(updates: &[u64], idx: u64) -> bool {
    updates.iter().any(|&u| u < idx)
}

pub async fn v4revertadj(network: String, start: u64, blocks: u64, commit: String, tolerance_bps: u64, seed: u64, out: PathBuf) -> anyhow::Result<()> {
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
    let mut tx_index: BTreeMap<B256, u64> = BTreeMap::new();
    let mut a = start;
    while a <= last {
        let z = (a + 9_999).min(last);
        let f = Filter::new().address(spokes.clone()).event_signature(LiquidationCall::SIGNATURE_HASH).from_block(a).to_block(z);
        for l in logs_retry(&p, &f).await? {
            let (Some(h), Some(b), Some(ti)) = (l.transaction_hash, l.block_number, l.transaction_index) else {
                anyhow::bail!("ethereum eth_getLogs returned a log without position fields");
            };
            let ev = LiquidationCall::decode_log(&l.inner).map_err(|_| anyhow::anyhow!("ethereum eth_getLogs returned an undecodable LiquidationCall"))?;
            let d = &ev.data;
            tx_index.insert(h, ti);
            txs.entry(h).or_default().push((b, l.address(), d.user, d.collateralReserveId, d.debtReserveId, d.debtAmountRestored, d.collateralAmountRemoved));
        }
        a = z + 1;
    }
    let tol = tolerance_bps as i128;
    let mut updates: BTreeMap<u64, Vec<u64>> = BTreeMap::new();
    let (mut reverting, mut rev_pre, mut other, mut other_pre, mut above_n1, mut above_pre, mut unreadable) = (0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64);
    for (h, evs) in &txs {
        let blk = evs[0].0;
        if !updates.contains_key(&blk) {
            let f = Filter::new().event_signature(AnswerUpdated::SIGNATURE_HASH).from_block(blk).to_block(blk);
            let idxs: Vec<u64> = logs_retry(&p, &f).await?.iter().filter_map(|l| l.transaction_index).collect();
            updates.insert(blk, idxs);
        }
        let pre = preceded(&updates[&blk], tx_index[h]);
        let mut still: Vec<&Ev> = Vec::new();
        for ev in evs {
            if bonus_reverts(&p, ev, ev.3).await && bonus_reverts(&p, ev, ev.4).await {
                still.push(ev);
            }
        }
        if still.is_empty() {
            other += 1;
            other_pre += pre as u64;
            continue;
        }
        reverting += 1;
        rev_pre += pre as u64;
        let (mut over_n1, mut fail) = (false, false);
        for ev in &still {
            match event_bonus(&p, ev, ev.0 - 1).await {
                Ok((l, m)) => over_n1 |= exceeds(l, m, tol),
                Err(_) => fail = true,
            }
        }
        if fail {
            unreadable += 1;
        } else if over_n1 {
            above_n1 += 1;
            above_pre += pre as u64;
        }
    }
    let v = serde_json::json!({
        "reverting_with_preceding_update_tx_count": rev_pre,
        "reverting_tx_count": reverting,
        "other_with_preceding_update_tx_count": other_pre,
        "other_tx_count": other,
        "above_max_at_n_minus_1_tx_count": above_n1,
        "above_max_with_preceding_update_tx_count": above_pre,
        "unreadable_tx_count": unreadable,
        "tolerance_bps": tolerance_bps,
        "v4_liquidation_tx_count": txs.len(),
        "spoke_count": spokes.len(),
        "first_block": start, "last_block": last, "blocks": blocks,
        "address_book_commit": commit,
        "provenance": {
            "started_at": started_at,
            "seeds": [seed],
            "command": format!("v4revertadj --chainstack-network {network} --start {start} --blocks {blocks} --address-book-commit {commit} --tolerance-bps {tolerance_bps} --seed {seed} --out {}", out.display()),
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
    fn tolerance_edge_matches_cmp() {
        assert!(!exceeds(600, 500, 100));
        assert!(exceeds(601, 500, 100));
        assert_eq!(max_bonus_bps(10_500), Some(500));
    }

    #[test]
    fn preceded_needs_strictly_lower_index() {
        assert!(preceded(&[3, 9], 4));
        assert!(!preceded(&[4, 9], 4));
        assert!(!preceded(&[], 4));
    }
}
