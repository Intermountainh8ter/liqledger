//! `v4oracle`: share of distinct Aave V4 liquidation landing blocks that also hold a Chainlink AnswerUpdated log (any feed),
//! beside a seeded baseline sample of non-liquidation blocks from the same range. Parameters are fixed in
//! preregistration_additions.md. Reads only; the endpoint is never printed.

use crate::defcheck::chrono_now;
use crate::mainoracleimmediate::{chainstack_endpoint, logs_retry};
use crate::mainoracleimmediatesplit::wilson;
use crate::v4census::{fetch_source, parse_spokes, LiquidationCall};
use crate::AnswerUpdated;
use alloy::primitives::Address;
use alloy::providers::ProviderBuilder;
use alloy::rpc::types::Filter;
use alloy::sol_types::SolEvent;
use rand::{Rng, SeedableRng};
use std::collections::BTreeSet;
use std::path::PathBuf;

/// Up to `n` distinct blocks drawn with `seed` from [start, start + blocks), skipping `exclude`.
pub fn baseline_blocks(seed: u64, start: u64, blocks: u64, n: usize, exclude: &BTreeSet<u64>) -> Vec<u64> {
    let mut rng = rand_pcg::Pcg64Mcg::seed_from_u64(seed);
    let mut out = BTreeSet::new();
    let cap = blocks.saturating_sub(exclude.len() as u64) as usize;
    let want = n.min(cap);
    while out.len() < want {
        let b = start + rng.random_range(0..blocks);
        if !exclude.contains(&b) {
            out.insert(b);
        }
    }
    out.into_iter().collect()
}

/// Share and enrichment ratio; None when undefined.
pub fn enrichment(with: u64, n: u64, base_with: u64, base_n: u64) -> (Option<f64>, Option<f64>, Option<f64>) {
    let s = (n > 0).then(|| with as f64 / n as f64);
    let b = (base_n > 0).then(|| base_with as f64 / base_n as f64);
    let e = match (s, b) {
        (Some(s), Some(b)) if b > 0.0 => Some(s / b),
        _ => None,
    };
    (s, b, e)
}

async fn has_update(p: &impl alloy::providers::Provider, b: u64) -> anyhow::Result<bool> {
    let f = Filter::new().event_signature(AnswerUpdated::SIGNATURE_HASH).from_block(b).to_block(b);
    Ok(!logs_retry(p, &f).await?.is_empty())
}

pub async fn v4oracle(network: String, start: u64, blocks: u64, commit: String, baseline_n: usize, seed: u64, out: PathBuf) -> anyhow::Result<()> {
    let started = std::time::Instant::now();
    let started_at = chrono_now();
    anyhow::ensure!(blocks >= 1, "blocks must be at least 1");
    let last = start + blocks - 1;
    let found = parse_spokes(&fetch_source(&commit)?);
    let spokes: Vec<Address> = found.iter().filter_map(|(_, a)| a.parse().ok()).collect();
    anyhow::ensure!(!spokes.is_empty(), "no Spokes found at the pinned commit");
    let url = chainstack_endpoint(&network)?;
    let p = ProviderBuilder::new().connect_http(url.parse().map_err(|_| anyhow::anyhow!("bad {network} endpoint"))?);
    let mut liq_blocks = BTreeSet::new();
    let mut events = 0u64;
    let mut a = start;
    while a <= last {
        let z = (a + 9_999).min(last);
        let f = Filter::new().address(spokes.clone()).event_signature(LiquidationCall::SIGNATURE_HASH).from_block(a).to_block(z);
        for l in logs_retry(&p, &f).await? {
            events += 1;
            if let Some(b) = l.block_number {
                liq_blocks.insert(b);
            }
        }
        a = z + 1;
    }
    let mut with = 0u64;
    for b in &liq_blocks {
        if has_update(&p, *b).await? {
            with += 1;
        }
    }
    let base = baseline_blocks(seed, start, blocks, baseline_n, &liq_blocks);
    let mut base_with = 0u64;
    for b in &base {
        if has_update(&p, *b).await? {
            base_with += 1;
        }
    }
    let n = liq_blocks.len() as u64;
    let bn = base.len() as u64;
    let (share, bshare, enr) = enrichment(with, n, base_with, bn);
    let ci = wilson(with, n);
    let v = serde_json::json!({
        "v4_same_block_oracle_share": share,
        "wilson_low": ci.map(|c| c.0), "wilson_high": ci.map(|c| c.1),
        "v4_liquidation_blocks": n,
        "v4_liquidation_blocks_with_update": with,
        "v4_liquidation_event_count": events,
        "baseline_blocks": bn,
        "baseline_blocks_with_update": base_with,
        "baseline_share": bshare,
        "enrichment_vs_baseline": enr,
        "spoke_count": spokes.len(),
        "first_block": start, "last_block": last, "blocks": blocks,
        "address_book_commit": commit,
        "provenance": {
            "started_at": started_at,
            "seeds": [seed],
            "command": format!("v4oracle --chainstack-network {network} --start {start} --blocks {blocks} --address-book-commit {commit} --baseline-blocks {baseline_n} --seed {seed} --out {}", out.display()),
            "hardware": format!("{} on {} logical cpus", std::env::consts::ARCH, std::thread::available_parallelism().map(|x| x.get()).unwrap_or(1)),
            "wall_s": started.elapsed().as_secs_f64(),
            "data_sources": [
                {"url": format!("chainstack:{network} blocks {start} to {last} (logs)"), "license": "public chain data via the owner's Chainstack node"},
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
    fn baseline_is_seeded_distinct_and_excludes() {
        let ex: BTreeSet<u64> = (100..150).collect();
        let a = baseline_blocks(42, 100, 200, 30, &ex);
        assert_eq!(a, baseline_blocks(42, 100, 200, 30, &ex));
        assert_eq!(a.len(), 30);
        assert!(a.iter().all(|b| (100..300).contains(b) && !ex.contains(b)));
    }

    #[test]
    fn baseline_caps_at_available() {
        let ex: BTreeSet<u64> = (0..8).collect();
        assert_eq!(baseline_blocks(1, 0, 10, 50, &ex), vec![8, 9]);
    }

    #[test]
    fn enrichment_cases() {
        assert_eq!(enrichment(6, 10, 2, 10), (Some(0.6), Some(0.2), Some(0.6 / 0.2)));
        assert_eq!(enrichment(0, 0, 1, 10).0, None);
        assert_eq!(enrichment(3, 10, 0, 10).2, None);
    }
}
