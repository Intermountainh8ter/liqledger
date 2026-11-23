//! `v4bonusdiag`: repeats the v4bonuscollrid scan and, for each transaction whose Spoke bonus call fails, records the spoke and the call outcome.
//! Parameters are fixed in preregistration_additions.md. Reads only; the endpoint is never printed.

use crate::defcheck::chrono_now;
use crate::mainoracleimmediate::{chainstack_endpoint, logs_retry};
use crate::v4bonuscurve::IV4Bonus;
use crate::v4census::{fetch_source, parse_spokes, LiquidationCall};
use alloy::eips::BlockId;
use alloy::primitives::{Address, B256, U256};
use alloy::providers::ProviderBuilder;
use alloy::rpc::types::Filter;
use alloy::sol_types::SolEvent;
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Ok,
    Revert,
    Empty,
    Timeout,
    Transport,
    Decode,
}

impl Outcome {
    pub fn name(&self) -> &'static str {
        match self {
            Outcome::Ok => "ok",
            Outcome::Revert => "revert",
            Outcome::Empty => "empty return",
            Outcome::Timeout => "timeout",
            Outcome::Transport => "transport error",
            Outcome::Decode => "undecodable return",
        }
    }
}

pub fn classify(e: &alloy::contract::Error) -> Outcome {
    use alloy::contract::Error as E;
    use alloy::transports::RpcError;
    match e {
        E::ZeroData(..) => Outcome::Empty,
        E::AbiError(_) => Outcome::Decode,
        E::TransportError(RpcError::ErrorResp(_)) => Outcome::Revert,
        _ => Outcome::Transport,
    }
}

type Ev = (u64, Address, Address, U256, U256);

async fn bonus_outcome<P: alloy::providers::Provider>(p: &P, ev: &Ev) -> Option<Outcome> {
    let (b, spoke, user, crid, _) = *ev;
    let t = std::time::Duration::from_secs(30);
    let blk = BlockId::number(b - 1);
    let sp = IV4Bonus::new(spoke, p);
    let hf = tokio::time::timeout(t, sp.getUserAccountData(user).block(blk).call()).await.ok()?.ok()?.healthFactor;
    Some(match tokio::time::timeout(t, sp.getLiquidationBonus(crid, user, hf).block(blk).call()).await {
        Err(_) => Outcome::Timeout,
        Ok(Ok(_)) => Outcome::Ok,
        Ok(Err(e)) => classify(&e),
    })
}

pub async fn v4bonusdiag(network: String, start: u64, blocks: u64, commit: String, tolerance_bps: u64, seed: u64, out: PathBuf) -> anyhow::Result<()> {
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
    let (mut readable, mut unreadable) = (0u64, 0u64);
    let mut by_outcome: BTreeMap<&'static str, u64> = BTreeMap::new();
    let mut by_spoke: BTreeMap<String, BTreeMap<&'static str, u64>> = BTreeMap::new();
    let mut hf_unreadable = 0u64;
    for evs in txs.values() {
        // First failing event decides the transaction, as in v4bonuscurve.
        let mut fail: Option<(Address, Outcome)> = None;
        let mut hf_fail = false;
        for ev in evs {
            match bonus_outcome(&p, ev).await {
                None => hf_fail = true,
                Some(Outcome::Ok) => {}
                Some(o) => {
                    fail.get_or_insert((ev.1, o));
                }
            }
        }
        match fail {
            Some((sp, o)) => {
                unreadable += 1;
                *by_outcome.entry(o.name()).or_default() += 1;
                *by_spoke.entry(format!("{sp:?}")).or_default().entry(o.name()).or_default() += 1;
            }
            None if hf_fail => hf_unreadable += 1,
            None => readable += 1,
        }
    }
    let cnt = |k: &str| by_outcome.get(k).copied().unwrap_or(0);
    let revert_or_empty = cnt("revert") + cnt("empty return");
    let timeout_or_transport = cnt("timeout") + cnt("transport error");
    let v = serde_json::json!({
        "bonus_call_revert_or_empty_count": revert_or_empty,
        "bonus_call_timeout_or_transport_count": timeout_or_transport,
        "bonus_call_unreadable_total": unreadable,
        "bonus_call_readable_tx_count": readable,
        "health_factor_unreadable_tx_count": hf_unreadable,
        "unreadable_by_outcome": by_outcome,
        "unreadable_by_spoke": by_spoke,
        "bonus_reserve_id_argument": "collateral",
        "tolerance_bps": tolerance_bps,
        "v4_liquidation_tx_count": txs.len(),
        "spoke_count": spokes.len(),
        "first_block": start, "last_block": last, "blocks": blocks,
        "address_book_commit": commit,
        "provenance": {
            "started_at": started_at,
            "seeds": [seed],
            "command": format!("v4bonusdiag --chainstack-network {network} --start {start} --blocks {blocks} --address-book-commit {commit} --tolerance-bps {tolerance_bps} --seed {seed} --out {}", out.display()),
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
    fn outcome_names_distinct() {
        let all = [Outcome::Ok, Outcome::Revert, Outcome::Empty, Outcome::Timeout, Outcome::Transport, Outcome::Decode];
        let mut n: Vec<_> = all.iter().map(|o| o.name()).collect();
        n.sort();
        n.dedup();
        assert_eq!(n.len(), all.len());
    }

    #[test]
    fn classify_abi_error_is_decode() {
        let e = alloy::contract::Error::AbiError(alloy::sol_types::Error::Overrun.into());
        assert_eq!(classify(&e), Outcome::Decode);
    }

    #[test]
    fn classify_unknown_function_is_transport() {
        assert_eq!(classify(&alloy::contract::Error::UnknownFunction("x".into())), Outcome::Transport);
    }
}
