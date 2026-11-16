//! `denimdrift`: share of landed Base transactions whose eth_call success flag at the parent block differs from the
//! receipt status, split by whether an earlier same-block transaction targeted the same address. Parameters are fixed
//! in preregistration_additions.md. Reads only; the endpoint comes from an environment variable and is never printed.

use crate::defcheck::chrono_now;
use alloy::providers::{Provider, ProviderBuilder};
use rand::seq::SliceRandom;
use rand::SeedableRng;
use std::collections::HashSet;
use std::path::PathBuf;

const MIN_TOUCHED: u64 = 300;

#[derive(Clone, Debug, PartialEq)]
pub struct Cand {
    pub block: u64,
    pub hash: String,
    pub from: String,
    pub to: String,
    pub value: String,
    pub input: String,
    pub gas: String,
    pub touched: bool,
}

/// Non-system transactions with a call target, flagged touched when an earlier non-system transaction in the same
/// block targeted the same address.
pub fn candidates(block_no: u64, block: &serde_json::Value) -> Vec<Cand> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut out = Vec::new();
    let s = |v: &serde_json::Value| v.as_str().unwrap_or("").to_string();
    for tx in block["transactions"].as_array().map(|a| a.as_slice()).unwrap_or(&[]) {
        if tx["type"].as_str() == Some("0x7e") || tx["to"].is_null() {
            continue;
        }
        let to = s(&tx["to"]).to_lowercase();
        let touched = !seen.insert(to.clone());
        out.push(Cand {
            block: block_no, hash: s(&tx["hash"]), from: s(&tx["from"]), to,
            value: s(&tx["value"]), input: s(&tx["input"]), gas: s(&tx["gas"]), touched,
        });
    }
    out
}

pub fn share(div: u64, n: u64, min: u64) -> Option<f64> {
    if n < min { None } else { Some(div as f64 / n as f64) }
}

async fn retry(
    p: &impl Provider, method: &'static str, params: serde_json::Value,
) -> anyhow::Result<Option<serde_json::Value>> {
    let mut tries = 0;
    loop {
        let r: Result<serde_json::Value, _> = p.raw_request(method.into(), params.clone()).await;
        match r {
            Ok(v) => return Ok(Some(v)),
            Err(e) if format!("{e}").to_lowercase().contains("revert") && method == "eth_call" => return Ok(None),
            Err(_) if tries < 5 => {
                tries += 1;
                tokio::time::sleep(std::time::Duration::from_millis(500 * (1 << tries))).await;
            }
            Err(_) => anyhow::bail!("base {method} failed"),
        }
    }
}

async fn diverges(p: &impl Provider, c: &Cand) -> anyhow::Result<bool> {
    let rec = retry(p, "eth_getTransactionReceipt", serde_json::json!([c.hash])).await?;
    let landed_ok = rec.as_ref().and_then(|r| r["status"].as_str()) == Some("0x1");
    let call = serde_json::json!({"from": c.from, "to": c.to, "value": c.value, "input": c.input, "gas": c.gas});
    let res = retry(p, "eth_call", serde_json::json!([call, format!("0x{:x}", c.block - 1)])).await?;
    Ok(res.is_some() != landed_ok)
}

pub async fn denimdrift(
    rpc_env: String, post_start: u64, window: u64, max_touched: usize, max_untouched: usize, seed: u64, out: PathBuf,
) -> anyhow::Result<()> {
    let started = std::time::Instant::now();
    let started_at = chrono_now();
    anyhow::ensure!(window >= 1 && post_start >= 1, "bad window");
    let url = std::env::var(&rpc_env).map_err(|_| anyhow::anyhow!("env {rpc_env} not set"))?;
    let p = ProviderBuilder::new().connect_http(url.parse().map_err(|_| anyhow::anyhow!("bad endpoint in {rpc_env}"))?);
    let last = post_start + window - 1;
    let (mut touched, mut untouched) = (Vec::new(), Vec::new());
    for n in post_start..=last {
        let b =
            retry(&p, "eth_getBlockByNumber", serde_json::json!([format!("0x{n:x}"), true])).await?;
        let b = b.filter(|v| !v.is_null()).ok_or_else(|| anyhow::anyhow!("base block {n} missing"))?;
        for c in candidates(n, &b) {
            if c.touched { touched.push(c) } else { untouched.push(c) }
        }
    }
    let (pool_t, pool_u) = (touched.len(), untouched.len());
    let mut rng = rand_pcg::Pcg64Mcg::seed_from_u64(seed);
    touched.shuffle(&mut rng);
    untouched.shuffle(&mut rng);
    touched.truncate(max_touched);
    untouched.truncate(max_untouched);
    let (mut dt, mut du) = (0u64, 0u64);
    if touched.len() as u64 >= MIN_TOUCHED {
        for c in &touched { dt += diverges(&p, c).await? as u64; }
        for c in &untouched { du += diverges(&p, c).await? as u64; }
    }
    let nt = if touched.len() as u64 >= MIN_TOUCHED { touched.len() as u64 } else { 0 };
    let nu = if nt > 0 { untouched.len() as u64 } else { 0 };
    let st = share(dt, nt, MIN_TOUCHED);
    let su = share(du, nu, 1);
    let diff = st.zip(su).map(|(a, b)| a - b);
    let v = serde_json::json!({
        "ranges": [{
            "label": "post", "first_block": post_start, "last_block": last,
            "touched_pool": pool_t, "untouched_pool": pool_u,
            "touched_replayed": nt, "untouched_replayed": nu,
            "divergent_touched": dt, "divergent_untouched": du,
            "divergent_share_touched": st, "divergent_share_untouched": su,
            "difference": diff, "abstain": st.is_none(),
        }],
        "provenance": {
            "started_at": started_at,
            "seeds": [seed],
            "command": format!("denimdrift --rpc-env {rpc_env} --post-start {post_start} --window {window} --max-touched {max_touched} --max-untouched {max_untouched} --seed {seed} --out results/175-denimdrift.json"),
            "hardware": format!("{} on {} logical cpus", std::env::consts::ARCH, std::thread::available_parallelism().map(|x| x.get()).unwrap_or(1)),
            "wall_s": started.elapsed().as_secs_f64(),
            "data_sources": [
                {"url": format!("chainstack:base blocks {post_start} to {last} (blocks, receipts, eth_call at parent block)"),
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
    fn touched_flags_and_filters() {
        let b = serde_json::json!({"transactions": [
            {"type": "0x7e", "to": "0xaa"},
            {"type": "0x2", "to": "0xAA", "hash": "h1"},
            {"type": "0x2", "to": null, "hash": "h2"},
            {"type": "0x2", "to": "0xaa", "hash": "h3"},
            {"type": "0x2", "to": "0xbb", "hash": "h4"}]});
        let c = candidates(9, &b);
        let flags: Vec<(String, bool)> = c.iter().map(|x| (x.hash.clone(), x.touched)).collect();
        assert_eq!(flags, vec![("h1".into(), false), ("h3".into(), true), ("h4".into(), false)]);
    }

    #[test]
    fn abstain_below_min() {
        assert!(share(5, 299, MIN_TOUCHED).is_none());
        assert_eq!(share(30, 300, MIN_TOUCHED), Some(0.1));
    }
}
