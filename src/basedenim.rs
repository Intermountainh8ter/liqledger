//! `basedenim`: on Base, counts blocks per timestamp second and blocks holding only deposit-type (0x7e) system
//! transactions over a pinned range after the Denim activation. Parameters are fixed in preregistration_additions.md.
//! Reads only; the endpoint comes from the Chainstack platform API at run time and is never printed.

use crate::defcheck::chrono_now;
use crate::denimempty::is_empty_block;
use crate::mainoracleimmediate::chainstack_endpoint;
use alloy::providers::{Provider, ProviderBuilder};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// First block (by height order) whose timestamp equals its successor's, from (number, timestamp) pairs.
pub fn first_shared_timestamp(ts: &[(u64, u64)]) -> Option<u64> {
    ts.windows(2).find(|w| w[0].1 == w[1].1).map(|w| w[0].0)
}

/// (maximum blocks in one timestamp second, number of distinct seconds).
pub fn per_second(ts: &[(u64, u64)]) -> (u64, u64) {
    let mut m: BTreeMap<u64, u64> = BTreeMap::new();
    for (_, t) in ts {
        *m.entry(*t).or_default() += 1;
    }
    (m.values().copied().max().unwrap_or(0), m.len() as u64)
}

pub async fn basedenim(network: String, start: u64, blocks: u64, seed: u64, out: PathBuf) -> anyhow::Result<()> {
    let started = std::time::Instant::now();
    let started_at = chrono_now();
    anyhow::ensure!(blocks >= 1, "blocks must be at least 1");
    let url = chainstack_endpoint(&network)?;
    let p = ProviderBuilder::new().connect_http(url.parse().map_err(|_| anyhow::anyhow!("bad {network} endpoint"))?);
    let (mut ts, mut empty, mut unreadable) = (Vec::new(), 0u64, 0u64);
    for n in start..start + blocks {
        let mut tries = 0;
        let b = loop {
            let r: Result<serde_json::Value, _> = p.raw_request("eth_getBlockByNumber".into(), (format!("0x{n:x}"), true)).await;
            match r {
                Ok(v) if !v.is_null() => break Some(v),
                Ok(_) => break None,
                Err(_) if tries < 5 => {
                    tries += 1;
                    tokio::time::sleep(std::time::Duration::from_millis(500 * (1 << tries))).await;
                }
                Err(_) => break None,
            }
        };
        match b {
            Some(b) => {
                if is_empty_block(&b) {
                    empty += 1;
                }
                let t = b["timestamp"].as_str().and_then(|t| u64::from_str_radix(t.trim_start_matches("0x"), 16).ok());
                match t {
                    Some(t) => ts.push((n, t)),
                    None => unreadable += 1,
                }
            }
            None => unreadable += 1,
        }
    }
    let (max_per_sec, seconds) = per_second(&ts);
    let v = serde_json::json!({
        "first_block": start, "blocks_requested": blocks, "blocks_read": ts.len(),
        "first_300_blocks_system_only_count": empty,
        "system_only_share": empty as f64 / ts.len().max(1) as f64,
        "first_block_sharing_timestamp_with_successor": first_shared_timestamp(&ts),
        "max_blocks_in_one_timestamp_second": max_per_sec,
        "distinct_timestamp_seconds": seconds,
        "unreadable_blocks": unreadable,
        "provenance": {
            "started_at": started_at,
            "seeds": [seed],
            "command": format!("basedenim --chainstack-network {network} --start {start} --blocks {blocks} --seed {seed} --out {}", out.display()),
            "hardware": format!("{} on {} logical cpus", std::env::consts::ARCH, std::thread::available_parallelism().map(|x| x.get()).unwrap_or(1)),
            "wall_s": started.elapsed().as_secs_f64(),
            "data_sources": [{"url": format!("chainstack:{network} blocks {start} to {} (full blocks with transactions)", start + blocks - 1),
                              "license": "public chain data via the owner's Chainstack node"}],
        }
    });
    std::fs::write(&out, serde_json::to_string_pretty(&v)? + "\n")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_timestamp_found() {
        assert_eq!(first_shared_timestamp(&[(1, 10), (2, 11), (3, 11), (4, 11)]), Some(2));
        assert_eq!(first_shared_timestamp(&[(1, 10), (2, 12)]), None);
    }

    #[test]
    fn per_second_counts() {
        assert_eq!(per_second(&[(1, 10), (2, 11), (3, 11), (4, 11)]), (3, 2));
        assert_eq!(per_second(&[]), (0, 0));
    }
}
