//! `denimempty`: share of blocks with no non-system transactions, and the longest run of consecutive empty blocks, for a
//! post-boundary range and an equal-length pre-boundary control on Base. Parameters are fixed in
//! preregistration_additions.md. Reads only; the endpoint comes from an environment variable and is never printed.

use crate::defcheck::chrono_now;
use alloy::providers::{Provider, ProviderBuilder};
use std::path::PathBuf;

/// Whether a block holds no transaction other than deposit-type (0x7e) system transactions.
pub fn is_empty_block(block: &serde_json::Value) -> bool {
    block["transactions"].as_array().map(|a| a.as_slice()).unwrap_or(&[]).iter().all(|tx| {
        tx["type"].as_str().and_then(|t| u64::from_str_radix(t.trim_start_matches("0x"), 16).ok()) == Some(0x7e)
    })
}

/// (empty blocks, longest run of consecutive empty blocks) over per-block flags in height order.
pub fn empty_stats(flags: &[bool]) -> (u64, u64) {
    let (mut empty, mut best, mut run) = (0u64, 0u64, 0u64);
    for &f in flags {
        if f {
            empty += 1;
            run += 1;
            best = best.max(run);
        } else {
            run = 0;
        }
    }
    (empty, best)
}

async fn range(p: &impl Provider, from: u64, to: u64) -> anyhow::Result<Vec<bool>> {
    let mut flags = Vec::new();
    for n in from..=to {
        let mut tries = 0;
        let b = loop {
            let r: Result<serde_json::Value, _> =
                p.raw_request("eth_getBlockByNumber".into(), (format!("0x{n:x}"), true)).await;
            match r {
                Ok(v) if !v.is_null() => break v,
                Ok(_) => anyhow::bail!("base block {n} missing"),
                Err(_) if tries < 5 => {
                    tries += 1;
                    tokio::time::sleep(std::time::Duration::from_millis(500 * (1 << tries))).await;
                }
                Err(_) => anyhow::bail!("base eth_getBlockByNumber failed at {n}"),
            }
        };
        flags.push(is_empty_block(&b));
    }
    Ok(flags)
}

pub async fn denimempty(rpc_env: String, post_start: u64, pre_end: u64, window: u64, out: PathBuf) -> anyhow::Result<()> {
    let started = std::time::Instant::now();
    let started_at = chrono_now();
    anyhow::ensure!(post_start == pre_end + 1 && window >= 1, "boundary must be adjacent");
    let url = std::env::var(&rpc_env).map_err(|_| anyhow::anyhow!("env {rpc_env} not set"))?;
    let p = ProviderBuilder::new().connect_http(url.parse().map_err(|_| anyhow::anyhow!("bad endpoint in {rpc_env}"))?);
    let pre_first = pre_end + 1 - window;
    let post_last = post_start + window - 1;
    let spans = [(post_start, post_last, "post"), (pre_first, pre_end, "pre")];
    let mut ranges = Vec::new();
    for (a, z, label) in spans {
        let flags = range(&p, a, z).await?;
        let (e, run) = empty_stats(&flags);
        ranges.push(serde_json::json!({
            "label": label, "first_block": a, "last_block": z,
            "blocks": flags.len(), "empty_blocks": e, "longest_empty_run": run,
            "empty_block_share": e as f64 / flags.len() as f64,
        }));
    }
    let diff = ranges[0]["empty_block_share"].as_f64().unwrap() - ranges[1]["empty_block_share"].as_f64().unwrap();
    let v = serde_json::json!({
        "ranges": ranges,
        "post_minus_pre_share": diff,
        "provenance": {
            "started_at": started_at,
            "seeds": [7],
            "command": format!("denimempty --rpc-env {rpc_env} --post-start {post_start} --pre-end {pre_end} --window {window} --out results/176-denimempty.json"),
            "hardware": format!("{} on {} logical cpus", std::env::consts::ARCH, std::thread::available_parallelism().map(|x| x.get()).unwrap_or(1)),
            "wall_s": started.elapsed().as_secs_f64(),
            "data_sources": [
                {"url": format!("chainstack:base blocks {pre_first} to {post_last} (full blocks with transactions)"),
                 "license": "public chain data via the owner's Chainstack node"}
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
    fn empty_block_ignores_deposits() {
        assert!(is_empty_block(&serde_json::json!({"transactions": [{"type": "0x7e"}]})));
        assert!(is_empty_block(&serde_json::json!({"transactions": []})));
        assert!(!is_empty_block(&serde_json::json!({"transactions": [{"type": "0x7e"}, {"type": "0x2"}]})));
    }

    #[test]
    fn stats_longest_run() {
        assert_eq!(empty_stats(&[true, true, false, true, true, true, false]), (5, 3));
        assert_eq!(empty_stats(&[false, false]), (0, 0));
        assert_eq!(empty_stats(&[]), (0, 0));
    }
}
