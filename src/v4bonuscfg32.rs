//! `v4bonuscfg32`: like v4bonuscfg but with a uint32 dynamic config key for the dynamic getter (uint24 kept as control). Repeats the v4bonuspos scan and, for each transaction whose bonus call still fails with the debt reserve id, reads the Spoke
//! dynamic-reserve-config and liquidation-config getters at block N-1, the first keyed by the dynamic config key in the borrower's position.
//! Parameters are fixed in preregistration_additions.md. Reads only; the endpoint is never printed.

use crate::defcheck::chrono_now;
use crate::mainoracleimmediate::{chainstack_endpoint, logs_retry};
use crate::v4bonuscurve::IV4Bonus;
use crate::v4bonusdiag::{classify, Outcome};
use crate::v4census::{fetch_source, parse_spokes, LiquidationCall};
use alloy::eips::BlockId;
use alloy::primitives::{Address, B256, U256};
use alloy::providers::ProviderBuilder;
use alloy::rpc::types::Filter;
use alloy::sol;
use alloy::sol_types::SolEvent;
use std::collections::BTreeMap;
use std::path::PathBuf;

sol! {
    struct UserPos { uint256 drawnShares; uint256 premiumShares; int256 premiumOffsetRay; uint256 suppliedShares; uint24 dynamicConfigKey; }
    struct UserPos32 { uint256 drawnShares; uint256 premiumShares; int256 premiumOffsetRay; uint256 suppliedShares; uint32 dynamicConfigKey; }
    struct DynCfg { uint16 collateralFactor; uint32 maxLiquidationBonus; uint16 liquidationFee; }
    struct LiqCfg { uint128 targetHealthFactor; uint64 healthFactorForMaxBonus; uint16 liquidationBonusFactor; }
    #[sol(rpc)]
    interface IV4Cfg {
        function getUserPosition(uint256 reserveId, address user) external view returns (UserPos memory);
        function getDynamicReserveConfig(uint256 reserveId, uint24 dynamicConfigKey) external view returns (DynCfg memory);
        function getLiquidationConfig() external view returns (LiqCfg memory);
    }
    #[sol(rpc)]
    interface IV4Cfg32 {
        function getUserPosition(uint256 reserveId, address user) external view returns (UserPos32 memory);
        function getDynamicReserveConfig(uint256 reserveId, uint32 dynamicConfigKey) external view returns (DynCfg memory);
    }
}

type Ev = (u64, Address, Address, U256, U256);

/// A getter counts as answering only when it returned data.
pub fn answers(o: Outcome) -> bool {
    o == Outcome::Ok
}

/// Transaction verdict over its events: answering only if every event answered.
pub fn all_answer(outs: &[Outcome]) -> bool {
    !outs.is_empty() && outs.iter().all(|o| answers(*o))
}

fn out_of<T>(r: Result<Result<T, alloy::contract::Error>, tokio::time::error::Elapsed>) -> Outcome {
    match r {
        Err(_) => Outcome::Timeout,
        Ok(Ok(_)) => Outcome::Ok,
        Ok(Err(e)) => classify(&e),
    }
}

/// (uint24 dynamic getter outcome, uint32 dynamic getter outcome, liquidation config getter outcome) for one event at block N-1.
async fn cfg_outcomes<P: alloy::providers::Provider>(p: &P, ev: &Ev) -> (Outcome, Outcome, Outcome) {
    let (b, spoke, user, crid, _) = *ev;
    let t = std::time::Duration::from_secs(30);
    let blk = BlockId::number(b - 1);
    let sp = IV4Cfg::new(spoke, p);
    let liq = out_of(tokio::time::timeout(t, sp.getLiquidationConfig().block(blk).call()).await);
    let dynamic = match tokio::time::timeout(t, sp.getUserPosition(crid, user).block(blk).call()).await {
        Ok(Ok(pos)) => out_of(tokio::time::timeout(t, sp.getDynamicReserveConfig(crid, pos.dynamicConfigKey).block(blk).call()).await),
        other => out_of(other),
    };
    let sp32 = IV4Cfg32::new(spoke, p);
    let dynamic32 = match tokio::time::timeout(t, sp32.getUserPosition(crid, user).block(blk).call()).await {
        Ok(Ok(pos)) => out_of(tokio::time::timeout(t, sp32.getDynamicReserveConfig(crid, pos.dynamicConfigKey).block(blk).call()).await),
        other => out_of(other),
    };
    (dynamic, dynamic32, liq)
}

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

pub async fn v4bonuscfg32(network: String, start: u64, blocks: u64, commit: String, seed: u64, out: PathBuf) -> anyhow::Result<()> {
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
    let (mut reverting, mut dynamic_ok, mut dynamic32_ok, mut liq_ok, mut unreadable) = (0u64, 0u64, 0u64, 0u64, 0u64);
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
        let (mut dyn_outs, mut dyn32_outs, mut liq_outs) = (Vec::new(), Vec::new(), Vec::new());
        for ev in &still {
            let (d, d32, l) = cfg_outcomes(&p, ev).await;
            dyn_outs.push(d);
            dyn32_outs.push(d32);
            liq_outs.push(l);
        }
        let spoke = by_spoke.entry(format!("{:?}", still[0].1)).or_default();
        let unread = |o: &[Outcome]| o.iter().any(|x| matches!(x, Outcome::Timeout | Outcome::Transport));
        if unread(&dyn_outs) || unread(&dyn32_outs) || unread(&liq_outs) {
            unreadable += 1;
            *spoke.entry("unreadable").or_default() += 1;
        }
        if all_answer(&dyn_outs) {
            dynamic_ok += 1;
            *spoke.entry("dynamic-answering").or_default() += 1;
        } else {
            *spoke.entry("dynamic-not-answering").or_default() += 1;
        }
        if all_answer(&dyn32_outs) {
            dynamic32_ok += 1;
            *spoke.entry("dynamic-u32-answering").or_default() += 1;
        }
        if all_answer(&liq_outs) {
            liq_ok += 1;
            *spoke.entry("liquidation-answering").or_default() += 1;
        }
    }
    let v = serde_json::json!({
        "dynamic_config_u32_answering_tx_count": dynamic32_ok,
        "dynamic_config_u24_control_answering_tx_count": dynamic_ok,
        "liquidation_config_answering_tx_count": liq_ok,
        "unreadable_tx_count": unreadable,
        "reverting_tx_count": reverting,
        "config_by_spoke": by_spoke,
        "config_read": "getDynamicReserveConfig with uint32 key and uint24 control (collateral reserve id, key from getUserPosition) and getLiquidationConfig() at block N-1",
        "v4_liquidation_tx_count": txs.len(),
        "spoke_count": spokes.len(),
        "first_block": start, "last_block": last, "blocks": blocks,
        "address_book_commit": commit,
        "provenance": {
            "started_at": started_at,
            "seeds": [seed],
            "command": format!("v4bonuscfg32 --chainstack-network {network} --start {start} --blocks {blocks} --address-book-commit {commit} --seed {seed} --out {}", out.display()),
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
    fn all_answer_needs_every_event() {
        assert!(all_answer(&[Outcome::Ok, Outcome::Ok]));
        assert!(!all_answer(&[Outcome::Ok, Outcome::Revert]));
        assert!(!all_answer(&[]));
    }
}
