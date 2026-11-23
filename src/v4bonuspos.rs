//! `v4bonuspos`: repeats the v4bonusdebtrid scan and, for each transaction whose bonus call still fails with the debt reserve id,
//! reads the borrower's supplied collateral in the liquidated reserve at block N-1. Parameters are fixed in preregistration_additions.md. Reads only; the endpoint is never printed.

use crate::defcheck::chrono_now;
use crate::mainoracleimmediate::{chainstack_endpoint, logs_retry};
use crate::v4bonuscurve::IV4Bonus;
use alloy::sol;
use crate::v4bonusdiag::{classify, Outcome};
use crate::v4census::{fetch_source, parse_spokes, LiquidationCall};
use alloy::eips::BlockId;
use alloy::primitives::{Address, B256, U256};
use alloy::providers::ProviderBuilder;
use alloy::rpc::types::Filter;
use alloy::sol_types::SolEvent;
use std::collections::BTreeMap;
use std::path::PathBuf;

sol! {
    #[sol(rpc)]
    interface IV4Pos {
        function getUserSuppliedAssets(uint256 reserveId, address user) external view returns (uint256);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pos {
    NonZero,
    Zero,
    Unreadable,
}

/// Combine per-event position reads into one transaction verdict: any unreadable read dominates, then any zero.
pub fn combine(reads: &[Pos]) -> Pos {
    if reads.contains(&Pos::Unreadable) {
        Pos::Unreadable
    } else if reads.contains(&Pos::Zero) {
        Pos::Zero
    } else {
        Pos::NonZero
    }
}

async fn position<P: alloy::providers::Provider>(p: &P, ev: &Ev) -> Pos {
    let (b, spoke, user, crid, _) = *ev;
    let t = std::time::Duration::from_secs(30);
    let r = tokio::time::timeout(t, IV4Pos::new(spoke, p).getUserSuppliedAssets(crid, user).block(BlockId::number(b - 1)).call()).await;
    match r {
        Ok(Ok(v)) if v.is_zero() => Pos::Zero,
        Ok(Ok(_)) => Pos::NonZero,
        _ => Pos::Unreadable,
    }
}

type Ev = (u64, Address, Address, U256, U256);

/// Outcome of the bonus call with the given reserve id; None when the health factor is unreadable.
async fn outcome_with<P: alloy::providers::Provider>(p: &P, ev: &Ev, rid: U256) -> Option<Outcome> {
    let (b, spoke, user, _, _) = *ev;
    let t = std::time::Duration::from_secs(30);
    let blk = BlockId::number(b - 1);
    let sp = IV4Bonus::new(spoke, p);
    let hf = tokio::time::timeout(t, sp.getUserAccountData(user).block(blk).call()).await.ok()?.ok()?.healthFactor;
    Some(match tokio::time::timeout(t, sp.getLiquidationBonus(rid, user, hf).block(blk).call()).await {
        Err(_) => Outcome::Timeout,
        Ok(Ok(_)) => Outcome::Ok,
        Ok(Err(e)) => classify(&e),
    })
}

/// Reserve id used by the retry: the debt id.
pub fn retry_rid(ev: &Ev) -> U256 {
    ev.4
}

pub async fn v4bonuspos(network: String, start: u64, blocks: u64, commit: String, seed: u64, out: PathBuf) -> anyhow::Result<()> {
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
            txs.entry(h).or_default().push((b, l.address(), d.user, d.collateralReserveId, d.debtReserveId));
        }
        a = z + 1;
    }
    let (mut reverting, mut nonzero, mut zero, mut unreadable) = (0u64, 0u64, 0u64, 0u64);
    let mut by_spoke: BTreeMap<String, BTreeMap<&'static str, u64>> = BTreeMap::new();
    for evs in txs.values() {
        let mut still: Vec<&Ev> = Vec::new();
        for ev in evs {
            if matches!(outcome_with(&p, ev, ev.3).await, None | Some(Outcome::Ok)) {
                continue;
            }
            if !matches!(outcome_with(&p, ev, retry_rid(ev)).await, None | Some(Outcome::Ok)) {
                still.push(ev);
            }
        }
        if still.is_empty() {
            continue;
        }
        reverting += 1;
        let mut reads = Vec::new();
        for ev in &still {
            reads.push(position(&p, ev).await);
        }
        let v = combine(&reads);
        let name = match v {
            Pos::NonZero => { nonzero += 1; "non-zero" }
            Pos::Zero => { zero += 1; "zero" }
            Pos::Unreadable => { unreadable += 1; "unreadable" }
        };
        *by_spoke.entry(format!("{:?}", still[0].1)).or_default().entry(name).or_default() += 1;
    }
    let v = serde_json::json!({
        "position_nonzero_tx_count": nonzero,
        "position_zero_tx_count": zero,
        "position_unreadable_tx_count": unreadable,
        "reverting_tx_count": reverting,
        "position_by_spoke": by_spoke,
        "position_read": "getUserSuppliedAssets(collateral reserve id, borrower) at block N-1",
        "v4_liquidation_tx_count": txs.len(),
        "spoke_count": spokes.len(),
        "first_block": start, "last_block": last, "blocks": blocks,
        "address_book_commit": commit,
        "provenance": {
            "started_at": started_at,
            "seeds": [seed],
            "command": format!("v4bonuspos --chainstack-network {network} --start {start} --blocks {blocks} --address-book-commit {commit} --seed {seed} --out {}", out.display()),
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
    fn retry_uses_debt_id() {
        let ev: Ev = (5, Address::ZERO, Address::ZERO, U256::from(1u8), U256::from(2u8));
        assert_eq!(retry_rid(&ev), U256::from(2u8));
    }

    #[test]
    fn combine_orders_verdicts() {
        assert_eq!(combine(&[Pos::NonZero, Pos::NonZero]), Pos::NonZero);
        assert_eq!(combine(&[Pos::NonZero, Pos::Zero]), Pos::Zero);
        assert_eq!(combine(&[Pos::Zero, Pos::Unreadable]), Pos::Unreadable);
    }
}
