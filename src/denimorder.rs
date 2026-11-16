//! `denimorder`: share of adjacent non-system transaction pairs whose effective priority fee is non-increasing, for a
//! post-boundary range and an equal-length pre-boundary control on Base. Parameters are fixed in
//! preregistration_additions.md. Reads only; the endpoint comes from an environment variable and is never printed.

use crate::defcheck::chrono_now;
use alloy::providers::{Provider, ProviderBuilder};
use std::path::PathBuf;

const MIN_PAIRS: u64 = 1000;

/// Effective priority fee in wei: min(tip cap, fee cap - base fee) for dynamic-fee, gas price - base fee for legacy.
/// None when the fee cap is below the base fee.
pub fn effective_tip(max_priority: Option<u128>, max_fee_or_price: u128, base_fee: u128) -> Option<u128> {
    let room = max_fee_or_price.checked_sub(base_fee)?;
    Some(max_priority.map_or(room, |t| t.min(room)))
}

/// (ordered pairs, total pairs) over one block's tips in landed order.
pub fn pair_counts(tips: &[u128]) -> (u64, u64) {
    let total = tips.len().saturating_sub(1) as u64;
    let ordered = tips.windows(2).filter(|w| w[1] <= w[0]).count() as u64;
    (ordered, total)
}

pub fn share(ordered: u64, total: u64) -> Option<f64> {
    if total < MIN_PAIRS { None } else { Some(ordered as f64 / total as f64) }
}

fn hex_u128(v: &serde_json::Value) -> Option<u128> {
    u128::from_str_radix(v.as_str()?.trim_start_matches("0x"), 16).ok()
}

/// Effective tips of one block's non-system transactions, from raw JSON so deposit-type transactions parse.
pub fn block_tips(block: &serde_json::Value) -> (Vec<u128>, u64) {
    let base = hex_u128(&block["baseFeePerGas"]).unwrap_or(0);
    let (mut tips, mut skipped) = (Vec::new(), 0u64);
    for tx in block["transactions"].as_array().map(|a| a.as_slice()).unwrap_or(&[]) {
        if hex_u128(&tx["type"]) == Some(0x7e) {
            continue;
        }
        let prio = hex_u128(&tx["maxPriorityFeePerGas"]);
        let cap = hex_u128(&tx["maxFeePerGas"]).or_else(|| hex_u128(&tx["gasPrice"]));
        match cap.and_then(|c| effective_tip(prio, c, base)) {
            Some(t) => tips.push(t),
            None => skipped += 1,
        }
    }
    (tips, skipped)
}

async fn range(p: &impl Provider, from: u64, to: u64) -> anyhow::Result<(u64, u64, u64)> {
    let (mut ord, mut tot, mut skipped) = (0u64, 0u64, 0u64);
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
        let (tips, sk) = block_tips(&b);
        skipped += sk;
        let (o, t) = pair_counts(&tips);
        ord += o;
        tot += t;
    }
    Ok((ord, tot, skipped))
}

pub async fn denimorder(rpc_env: String, post_start: u64, pre_end: u64, window: u64, out: PathBuf) -> anyhow::Result<()> {
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
        let (o, t, sk) = range(&p, a, z).await?;
        let s = share(o, t);
        ranges.push(serde_json::json!({
            "label": label, "first_block": a, "last_block": z,
            "ordered_pairs": o, "total_pairs": t, "skipped_below_base_fee": sk,
            "ordered_pair_share": s, "abstain": s.is_none(),
        }));
    }
    let diff = match (ranges[0]["ordered_pair_share"].as_f64(), ranges[1]["ordered_pair_share"].as_f64()) {
        (Some(a), Some(b)) => Some(a - b),
        _ => None,
    };
    let v = serde_json::json!({
        "ranges": ranges,
        "post_minus_pre_share": diff,
        "provenance": {
            "started_at": started_at,
            "seeds": [7],
            "command": format!("denimorder --rpc-env {rpc_env} --post-start {post_start} --pre-end {pre_end} --window {window} --out results/172-denimorder.json"),
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
    fn tip_dynamic_and_legacy() {
        assert_eq!(effective_tip(Some(5), 100, 98), Some(2));
        assert_eq!(effective_tip(Some(1), 100, 98), Some(1));
        assert_eq!(effective_tip(None, 100, 98), Some(2));
        assert_eq!(effective_tip(Some(5), 90, 98), None);
    }

    #[test]
    fn block_tips_skips_deposits() {
        let b = serde_json::json!({"baseFeePerGas": "0x64", "transactions": [
            {"type": "0x7e"},
            {"type": "0x2", "maxPriorityFeePerGas": "0x5", "maxFeePerGas": "0x68"},
            {"type": "0x0", "gasPrice": "0x66"},
            {"type": "0x0", "gasPrice": "0x10"}]});
        assert_eq!(block_tips(&b), (vec![4, 2], 1));
    }

    #[test]
    fn pairs_counted() {
        assert_eq!(pair_counts(&[5, 5, 3, 4]), (2, 3));
        assert_eq!(pair_counts(&[7]), (0, 0));
    }

    #[test]
    fn abstain_below_min_pairs() {
        assert!(share(10, 999).is_none());
        assert_eq!(share(900, 1000), Some(0.9));
    }
}
