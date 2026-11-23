//! `v4replayfail`: repeat the debug_traceCall replays of `v4reexec` and classify each failed replay by the node's error field.
//! Parameters are fixed in preregistration_additions.md. Reads only; the endpoint is never printed and error text is never written.

use crate::defcheck::chrono_now;
use crate::mainoracleimmediate::{chainstack_endpoint, logs_retry};
use crate::mainoracleimmediatesplit::wilson;
use crate::tracereplay::raw;
use crate::v4census::{fetch_source, parse_spokes, LiquidationCall};
use alloy::primitives::{Address, B256};
use alloy::providers::{Provider, ProviderBuilder};
use alloy::rpc::types::Filter;
use alloy::sol_types::SolEvent;
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum FailClass {
    Revert,
    StateUnavailable,
    UnsupportedMethod,
    Other,
}

/// Classify an error message from the node (a trace-level `error` field or a JSON-RPC error message).
pub fn classify(msg: &str) -> FailClass {
    let m = msg.to_ascii_lowercase();
    if m.contains("revert") || m.contains("invalid opcode") || m.contains("out of gas") {
        FailClass::Revert
    } else if m.contains("missing trie node") || m.contains("state") && (m.contains("not available") || m.contains("pruned") || m.contains("not found")) || m.contains("historical") || m.contains("header not found") {
        FailClass::StateUnavailable
    } else if m.contains("method not found") || m.contains("not supported") || m.contains("does not exist") || m.contains("unsupported") || m.contains("not available") {
        FailClass::UnsupportedMethod
    } else {
        FailClass::Other
    }
}

/// One debug_traceCall; Ok(trace) on a clean result, Err(class) otherwise. Rate-limit answers are retried with back-off.
async fn trace_once(p: &impl Provider, params: serde_json::Value) -> Result<serde_json::Value, FailClass> {
    let mut tries = 0u32;
    loop {
        match p.raw_request::<_, serde_json::Value>("debug_traceCall".into(), params.clone()).await {
            Ok(t) if t.is_null() => return Err(FailClass::Other),
            Ok(t) => {
                return match t["error"].as_str() {
                    Some(e) => Err(classify(e)),
                    None if t["error"].is_null() => Ok(t),
                    None => Err(FailClass::Other),
                }
            }
            Err(e) => {
                let msg = e.as_error_resp().map(|r| r.message.to_string());
                let transient = match &msg {
                    Some(m) => m.contains("429") || m.to_ascii_lowercase().contains("rate limit"),
                    None => true,
                };
                if transient && tries < 4 {
                    tries += 1;
                    tokio::time::sleep(std::time::Duration::from_millis(500 * (1 << tries))).await;
                    continue;
                }
                return Err(msg.map(|m| classify(&m)).unwrap_or(FailClass::Other));
            }
        }
    }
}

/// (failed, reverts, state-unavailable, unsupported-method, other) from classes.
pub fn tally(classes: &[FailClass]) -> (u64, u64, u64, u64, u64) {
    let mut r = (classes.len() as u64, 0, 0, 0, 0);
    for c in classes {
        match c {
            FailClass::Revert => r.1 += 1,
            FailClass::StateUnavailable => r.2 += 1,
            FailClass::UnsupportedMethod => r.3 += 1,
            FailClass::Other => r.4 += 1,
        }
    }
    r
}

pub async fn v4replayfail(network: String, start: u64, blocks: u64, commit: String, seed: u64, out: PathBuf) -> anyhow::Result<()> {
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
    let mut classes: Vec<FailClass> = Vec::new();
    let mut fail_index_zero = 0u64;
    let mut replayed_ok = 0u64;
    for (h, (b, ti)) in txs.iter() {
        let hs = format!("{h:#x}");
        let tx = raw(&p, "eth_getTransactionByHash", serde_json::json!([hs])).await?;
        let call = serde_json::json!({"from": tx["from"], "to": tx["to"], "gas": tx["gas"], "value": tx["value"], "data": tx["input"]});
        let cfg = serde_json::json!({"tracer": "callTracer", "tracerConfig": {"withLog": true}});
        match trace_once(&p, serde_json::json!([call, format!("0x{:x}", *b - 1), cfg])).await {
            Ok(_) => replayed_ok += 1,
            Err(c) => {
                classes.push(c);
                fail_index_zero += (*ti == 0) as u64;
            }
        }
    }
    let (failed, rv, su, um, ot) = tally(&classes);
    let share = |k: u64| (failed > 0).then(|| k as f64 / failed as f64);
    let ci = wilson(rv, failed);
    let v = serde_json::json!({
        "revert_share_of_failed": share(rv),
        "wilson_low": ci.map(|c| c.0), "wilson_high": ci.map(|c| c.1),
        "failed_replay_count": failed,
        "replayed_ok_count": replayed_ok,
        "revert_count": rv, "state_unavailable_count": su, "unsupported_method_count": um, "other_count": ot,
        "other_share_of_failed": share(ot),
        "failed_at_index_zero_count": fail_index_zero,
        "v4_liquidation_tx_count": txs.len(),
        "spoke_count": spokes.len(),
        "first_block": start, "last_block": last, "blocks": blocks,
        "address_book_commit": commit,
        "provenance": {
            "started_at": started_at,
            "seeds": [seed],
            "command": format!("v4replayfail --chainstack-network {network} --start {start} --blocks {blocks} --address-book-commit {commit} --seed {seed} --out {}", out.display()),
            "hardware": format!("{} on {} logical cpus", std::env::consts::ARCH, std::thread::available_parallelism().map(|x| x.get()).unwrap_or(1)),
            "wall_s": started.elapsed().as_secs_f64(),
            "data_sources": [
                {"url": format!("chainstack:{network} blocks {start} to {last} (logs, debug_traceCall at parent block)"), "license": "public chain data via the owner's Chainstack node"},
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
    fn classifies_errors() {
        assert_eq!(classify("execution reverted"), FailClass::Revert);
        assert_eq!(classify("Execution Reverted: HF_OK"), FailClass::Revert);
        assert_eq!(classify("missing trie node abc (path )"), FailClass::StateUnavailable);
        assert_eq!(classify("the method debug_traceCall does not exist/is not available"), FailClass::UnsupportedMethod);
        assert_eq!(classify("something odd"), FailClass::Other);
    }

    #[test]
    fn tallies_classes() {
        let c = [FailClass::Revert, FailClass::Revert, FailClass::Other, FailClass::StateUnavailable, FailClass::UnsupportedMethod];
        assert_eq!(tally(&c), (5, 2, 1, 1, 1));
        assert_eq!(tally(&[]), (0, 0, 0, 0, 0));
    }
}
