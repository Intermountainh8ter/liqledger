//! `denimwin`: wall time of equal-length block-count windows before and after a pinned boundary on Base, from block
//! headers alone. Parameters are fixed in preregistration_additions.md. Reads only; the endpoint comes from an
//! environment variable and is never printed.

use crate::defcheck::chrono_now;
use alloy::eips::BlockNumberOrTag;
use alloy::providers::{Provider, ProviderBuilder};
use std::path::PathBuf;

#[derive(Debug, PartialEq)]
pub struct Window {
    pub first_block: u64,
    pub last_block: u64,
    pub wall_s: u64,
    pub median_interval_s: f64,
}

/// `ts` holds consecutive block timestamps starting at `first_block`.
pub fn window_stats(first_block: u64, ts: &[u64]) -> Option<Window> {
    if ts.len() < 2 || ts.windows(2).any(|w| w[1] < w[0]) {
        return None;
    }
    let mut d: Vec<u64> = ts.windows(2).map(|w| w[1] - w[0]).collect();
    d.sort_unstable();
    let n = d.len();
    let median = if n % 2 == 1 { d[n / 2] as f64 } else { (d[n / 2 - 1] + d[n / 2]) as f64 / 2.0 };
    Some(Window {
        first_block,
        last_block: first_block + ts.len() as u64 - 1,
        wall_s: ts[ts.len() - 1] - ts[0],
        median_interval_s: median,
    })
}

pub fn ratio(pre: &Window, post: &Window) -> Option<f64> {
    if pre.wall_s == 0 {
        None
    } else {
        Some(post.wall_s as f64 / pre.wall_s as f64)
    }
}

async fn timestamps(p: &impl Provider, from: u64, to: u64) -> anyhow::Result<Vec<u64>> {
    let mut out = Vec::new();
    for n in from..=to {
        let mut tries = 0;
        let b = loop {
            match p.get_block_by_number(BlockNumberOrTag::Number(n)).await {
                Ok(Some(b)) => break b,
                Ok(None) => anyhow::bail!("base header {n} missing"),
                Err(_) if tries < 5 => {
                    tries += 1;
                    tokio::time::sleep(std::time::Duration::from_millis(500 * (1 << tries))).await;
                }
                Err(_) => anyhow::bail!("base eth_getBlockByNumber failed at {n}"),
            }
        };
        out.push(b.header.timestamp);
    }
    Ok(out)
}

pub async fn denimwin(rpc_env: String, pre_end: u64, post_start: u64, window: u64, out: PathBuf) -> anyhow::Result<()> {
    let started = std::time::Instant::now();
    let started_at = chrono_now();
    anyhow::ensure!(post_start == pre_end + 1 && window >= 2, "boundary must be adjacent and window >= 2");
    let url = std::env::var(&rpc_env).map_err(|_| anyhow::anyhow!("env {rpc_env} not set"))?;
    let p = ProviderBuilder::new().connect_http(url.parse().map_err(|_| anyhow::anyhow!("bad endpoint in {rpc_env}"))?);
    let pre_first = pre_end + 1 - window;
    let post_last = post_start + window - 1;
    let pre_ts = timestamps(&p, pre_first, pre_end).await?;
    let post_ts = timestamps(&p, post_start, post_last).await?;
    let pre = window_stats(pre_first, &pre_ts);
    let post = window_stats(post_start, &post_ts);
    let (pre, post) = match (pre, post) {
        (Some(a), Some(b)) => (a, b),
        _ => anyhow::bail!("headers not monotone or too few"),
    };
    let r = ratio(&pre, &post);
    let w = |x: &Window| serde_json::json!({
        "first_block": x.first_block, "last_block": x.last_block,
        "wall_s": x.wall_s, "median_interval_s": x.median_interval_s,
        "mean_interval_s": x.wall_s as f64 / (window - 1) as f64,
    });
    let v = serde_json::json!({
        "boundary": {"pre_end_block": pre_end, "post_start_block": post_start,
            "pre_end_timestamp": pre_ts[pre_ts.len() - 1], "post_start_timestamp": post_ts[0]},
        "windows": [{
            "block_count": window,
            "pre": w(&pre), "post": w(&post),
            "wall_time_ratio": r,
            "abstain": r.is_none(),
        }],
        "provenance": {
            "started_at": started_at,
            "seeds": [7],
            "command": format!("denimwin --rpc-env {rpc_env} --pre-end {pre_end} --post-start {post_start} --window {window} --out results/171-denimwin.json"),
            "hardware": format!("{} on {} logical cpus", std::env::consts::ARCH, std::thread::available_parallelism().map(|x| x.get()).unwrap_or(1)),
            "wall_s": started.elapsed().as_secs_f64(),
            "data_sources": [
                {"url": format!("chainstack:base blocks {pre_first} to {post_last} (headers only)"),
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
    fn window_stats_basic() {
        let w = window_stats(10, &[100, 102, 104, 106]).unwrap();
        assert_eq!(w.wall_s, 6);
        assert_eq!(w.last_block, 13);
        assert_eq!(w.median_interval_s, 2.0);
    }

    #[test]
    fn rejects_non_monotone_and_short() {
        assert!(window_stats(0, &[5]).is_none());
        assert!(window_stats(0, &[5, 4]).is_none());
    }

    #[test]
    fn ratio_and_abstain() {
        let pre = window_stats(0, &[0, 2, 4, 6, 8, 10]).unwrap();
        let post = window_stats(6, &[10, 10, 10, 11, 11, 11]).unwrap();
        assert!((ratio(&pre, &post).unwrap() - 0.1).abs() < 1e-12);
        let flat = window_stats(0, &[3, 3, 3]).unwrap();
        assert!(ratio(&flat, &post).is_none());
    }
}
