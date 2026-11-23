//! `v4revertflip`: for V4 liquidation replays that revert at parent-block state (result 321), repeat the replay at the landed block with
//! debug_traceCall's txIndex so earlier same-block transactions are applied first, and count how many no longer revert.
//! Parameters are fixed in preregistration_additions.md. Reads only; the endpoint is never printed and error text is never written.

use crate::defcheck::chrono_now;
use crate::mainoracleimmediate::{chainstack_endpoint, logs_retry};
use crate::mainoracleimmediatesplit::wilson;
use crate::tracereplay::raw;
use crate::v4census::{fetch_source, parse_spokes, LiquidationCall};
use crate::v4replayfail::{trace_once, FailClass};
use alloy::primitives::{Address, B256};
use alloy::providers::ProviderBuilder;
use alloy::rpc::types::Filter;
use alloy::sol_types::SolEvent;
use std::collections::BTreeMap;
use std::path::PathBuf;

/// (flipped to success, still reverting, abstained) from the landed-block replay outcomes of parent-state reverts.
pub fn flip_tally(outcomes: &[Result<(), FailClass>]) -> (u64, u64, u64) {
    let mut r = (0, 0, 0);
    for o in outcomes {
        match o {
            Ok(()) => r.0 += 1,
            Err(FailClass::Revert) => r.1 += 1,
            Err(_) => r.2 += 1,
        }
    }
    r
}

pub async fn v4revertflip(network: String, start: u64, blocks: u64, commit: String, seed: u64, out: PathBuf) -> anyhow::Result<()> {
    let started = std::time::Instant::now();
    let started_at = chrono_now();
    anyhow::ensure!(blocks >= 1, "blocks must be at least 1");
    let last = start + blocks - 1;
    let found = parse_spokes(&fetch_source(&commit)?);
    let spokes: Vec<Address> = found.iter().filter_map(|(_, a)| a.parse().ok()).collect();
    anyhow::ensure!(!spokes.is_empty(), "no Spokes found at the pinned commit");
    let url = chainstack_endpoint(&network)?;
    let p = ProviderBuilder::new().connect_http(url.parse().map_err(|_| anyhow::anyhow!("bad {network} endpoint"))?);
    let mut txs: BTreeMap<B256, (u64, u64)> = BTreeMap::new();
    let mut a = start;
    while a <= last {
        let z = (a + 9_999).min(last);
        let f = Filter::new().address(spokes.clone()).event_signature(LiquidationCall::SIGNATURE_HASH).from_block(a).to_block(z);
        for l in logs_retry(&p, &f).await? {
            let (Some(h), Some(b), Some(i)) = (l.transaction_hash, l.block_number, l.transaction_index) else {
                anyhow::bail!("ethereum eth_getLogs returned a log without position fields");
            };
            let e = txs.entry(h).or_insert((b, i));
            e.1 = e.1.min(i);
        }
        a = z + 1;
    }
    let mut reverting = 0u64;
    let mut outcomes: Vec<Result<(), FailClass>> = Vec::new();
    let mut flip_index_zero = 0u64;
    for (h, (b, ti)) in txs.iter() {
        let hs = format!("{h:#x}");
        let tx = raw(&p, "eth_getTransactionByHash", serde_json::json!([hs])).await?;
        let call = serde_json::json!({"from": tx["from"], "to": tx["to"], "gas": tx["gas"], "value": tx["value"], "data": tx["input"]});
        let cfg = serde_json::json!({"tracer": "callTracer", "tracerConfig": {"withLog": true}});
        if !matches!(trace_once(&p, serde_json::json!([call, format!("0x{:x}", *b - 1), cfg])).await, Err(FailClass::Revert)) {
            continue;
        }
        reverting += 1;
        let cfg2 = serde_json::json!({"tracer": "callTracer", "tracerConfig": {"withLog": true}, "txIndex": *ti});
        let o = trace_once(&p, serde_json::json!([call, format!("0x{:x}", *b), cfg2])).await.map(|_| ());
        flip_index_zero += (o.is_ok() && *ti == 0) as u64;
        outcomes.push(o);
    }
    let (flipped, still, abstain) = flip_tally(&outcomes);
    let share = (reverting > 0).then(|| flipped as f64 / reverting as f64);
    let ci = wilson(flipped, reverting);
    let v = serde_json::json!({
        "revert_flip_to_success_share": share,
        "wilson_low": ci.map(|c| c.0), "wilson_high": ci.map(|c| c.1),
        "reverting_count": reverting,
        "flipped_to_success_count": flipped,
        "still_reverting_count": still,
        "abstain_count": abstain,
        "flipped_at_index_zero_count": flip_index_zero,
        "v4_liquidation_tx_count": txs.len(),
        "spoke_count": spokes.len(),
        "first_block": start, "last_block": last, "blocks": blocks,
        "address_book_commit": commit,
        "provenance": {
            "started_at": started_at,
            "seeds": [seed],
            "command": format!("v4revertflip --chainstack-network {network} --start {start} --blocks {blocks} --address-book-commit {commit} --seed {seed} --out {}", out.display()),
            "hardware": format!("{} on {} logical cpus", std::env::consts::ARCH, std::thread::available_parallelism().map(|x| x.get()).unwrap_or(1)),
            "wall_s": started.elapsed().as_secs_f64(),
            "data_sources": [
                {"url": format!("chainstack:{network} blocks {start} to {last} (logs, debug_traceCall at parent block and at landed block with txIndex)"), "license": "public chain data via the owner's Chainstack node"},
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
    fn tallies_flips() {
        let o = [Ok(()), Ok(()), Err(FailClass::Revert), Err(FailClass::StateUnavailable), Err(FailClass::Other)];
        assert_eq!(flip_tally(&o), (2, 1, 2));
        assert_eq!(flip_tally(&[]), (0, 0, 0));
    }
}
