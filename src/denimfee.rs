//! `denimfee`: per-block gas limit, gas-used share of the limit, and base-fee direction for a post-boundary range and an
//! equal-length pre-boundary control on Base. Parameters are fixed in preregistration_additions.md. Reads headers only;
//! the endpoint comes from an environment variable and is never printed.

use crate::defcheck::chrono_now;
use alloy::providers::{Provider, ProviderBuilder};
use std::path::PathBuf;

fn hex(v: &serde_json::Value, k: &str) -> Option<u64> {
    u64::from_str_radix(v[k].as_str()?.trim_start_matches("0x"), 16).ok()
}

pub fn median(xs: &[u64]) -> f64 {
    if xs.is_empty() {
        return f64::NAN;
    }
    let mut s = xs.to_vec();
    s.sort_unstable();
    let m = s.len() / 2;
    if s.len() % 2 == 1 { s[m] as f64 } else { (s[m - 1] as f64 + s[m] as f64) / 2.0 }
}

/// Share of consecutive pairs where the base fee strictly rose.
pub fn rise_share(fees: &[u64]) -> f64 {
    if fees.len() < 2 {
        return f64::NAN;
    }
    let up = fees.windows(2).filter(|w| w[1] > w[0]).count();
    up as f64 / (fees.len() - 1) as f64
}

async fn range(p: &impl Provider, from: u64, to: u64) -> anyhow::Result<(Vec<u64>, Vec<u64>, Vec<u64>)> {
    let (mut limits, mut used, mut fees) = (vec![], vec![], vec![]);
    for n in from..=to {
        let mut tries = 0;
        let b = loop {
            let r: Result<serde_json::Value, _> =
                p.raw_request("eth_getBlockByNumber".into(), (format!("0x{n:x}"), false)).await;
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
        limits.push(hex(&b, "gasLimit").ok_or_else(|| anyhow::anyhow!("no gasLimit at {n}"))?);
        used.push(hex(&b, "gasUsed").ok_or_else(|| anyhow::anyhow!("no gasUsed at {n}"))?);
        fees.push(hex(&b, "baseFeePerGas").ok_or_else(|| anyhow::anyhow!("no baseFeePerGas at {n}"))?);
    }
    Ok((limits, used, fees))
}

pub async fn denimfee(rpc_env: String, post_start: u64, pre_end: u64, window: u64, out: PathBuf) -> anyhow::Result<()> {
    let started = std::time::Instant::now();
    let started_at = chrono_now();
    anyhow::ensure!(post_start == pre_end + 1 && window >= 2, "boundary must be adjacent");
    let url = std::env::var(&rpc_env).map_err(|_| anyhow::anyhow!("env {rpc_env} not set"))?;
    let p = ProviderBuilder::new().connect_http(url.parse().map_err(|_| anyhow::anyhow!("bad endpoint in {rpc_env}"))?);
    let pre_first = pre_end + 1 - window;
    let post_last = post_start + window - 1;
    let spans = [(post_start, post_last, "post"), (pre_first, pre_end, "pre")];
    let mut ranges = Vec::new();
    for (a, z, label) in spans {
        let (limits, used, fees) = range(&p, a, z).await?;
        let shares: Vec<f64> = limits.iter().zip(&used).map(|(l, u)| *u as f64 / *l as f64).collect();
        let mut sorted = shares.clone();
        sorted.sort_by(|x, y| x.partial_cmp(y).unwrap());
        let m = sorted.len() / 2;
        let med_share = if sorted.len() % 2 == 1 { sorted[m] } else { (sorted[m - 1] + sorted[m]) / 2.0 };
        ranges.push(serde_json::json!({
            "label": label, "first_block": a, "last_block": z, "blocks": limits.len(),
            "gas_limit_median": median(&limits),
            "gas_used_median": median(&used),
            "gas_used_share_median": med_share,
            "base_fee_median_wei": median(&fees),
            "base_fee_rise_share": rise_share(&fees),
        }));
    }
    let ratio = ranges[0]["gas_limit_median"].as_f64().unwrap() / ranges[1]["gas_limit_median"].as_f64().unwrap();
    let v = serde_json::json!({
        "ranges": ranges,
        "comparison": { "gas_limit_median_ratio": ratio },
        "provenance": {
            "started_at": started_at,
            "seeds": [7],
            "command": format!("denimfee --rpc-env {rpc_env} --post-start {post_start} --pre-end {pre_end} --window {window} --out results/177-denimfee.json"),
            "hardware": format!("{} on {} logical cpus", std::env::consts::ARCH, std::thread::available_parallelism().map(|x| x.get()).unwrap_or(1)),
            "wall_s": started.elapsed().as_secs_f64(),
            "data_sources": [
                {"url": format!("chainstack:base blocks {pre_first} to {post_last} (headers)"),
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
    fn median_odd_even() {
        assert_eq!(median(&[3, 1, 2]), 2.0);
        assert_eq!(median(&[4, 1, 2, 3]), 2.5);
        assert!(median(&[]).is_nan());
    }

    #[test]
    fn rise_share_counts_strict_rises() {
        assert_eq!(rise_share(&[1, 2, 2, 3]), 2.0 / 3.0);
        assert!(rise_share(&[1]).is_nan());
    }

    #[test]
    fn hex_parses() {
        assert_eq!(hex(&serde_json::json!({"gasLimit": "0x10"}), "gasLimit"), Some(16));
        assert_eq!(hex(&serde_json::json!({}), "gasLimit"), None);
    }
}
