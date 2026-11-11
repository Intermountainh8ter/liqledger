//! `remaindercheck`: does the ranking of builders by recapture rate depend on whether the denominator is the total bid
//! or the sum of named payees. Model fixed in preregistration_additions.md; no chain data is used.

use crate::defcheck::chrono_now;
use rand::{Rng, SeedableRng};
use std::path::PathBuf;

pub const BUILDERS: usize = 5;

/// Per-builder recapture rates: (over total bid, over named-payee sum).
pub fn rates(seed: u64, n: usize) -> Vec<[f64; 2]> {
    let mut rng = rand_pcg::Pcg64Mcg::seed_from_u64(seed);
    let cap: Vec<f64> = (0..BUILDERS).map(|_| rng.random_range(0.1..0.9)).collect();
    let rem: Vec<f64> = (0..BUILDERS).map(|_| rng.random_range(0.05..0.5)).collect();
    let mut sums = vec![[0.0f64; 3]; BUILDERS]; // recaptured, total bid, named sum
    for _ in 0..n {
        let b = rng.random_range(0..BUILDERS);
        let bid: f64 = rng.random_range(1.0..100.0);
        sums[b][0] += cap[b] * bid;
        sums[b][1] += bid;
        sums[b][2] += (1.0 - rem[b]) * bid;
    }
    sums.iter().map(|s| [s[0] / s[1], s[0] / s[2]]).collect()
}

/// Pairs ordered differently under the two denominators.
pub fn flips(r: &[[f64; 2]]) -> usize {
    let mut c = 0;
    for i in 0..r.len() {
        for j in i + 1..r.len() {
            if (r[i][0] - r[j][0]) * (r[i][1] - r[j][1]) < 0.0 {
                c += 1;
            }
        }
    }
    c
}

pub fn remaindercheck(seed: u64, n: usize, out: PathBuf) -> anyhow::Result<()> {
    let started = std::time::Instant::now();
    let started_at = chrono_now();
    let r = rates(seed, n);
    let v = serde_json::json!({
        "n_bids": n,
        "n_builders": BUILDERS,
        "n_pairs": BUILDERS * (BUILDERS - 1) / 2,
        "remainder": {
            "rank_flip_count": flips(&r),
            "builder_rates": r.iter().map(|x| serde_json::json!({
                "over_total_bid": x[0], "over_named_sum": x[1]})).collect::<Vec<_>>(),
        },
        "provenance": {
            "started_at": started_at,
            "seeds": [seed],
            "command": format!("cargo run --locked --release -- remaindercheck --seed {seed} --n {n} --out results/78-remaindercheck.json"),
            "hardware": format!("{} on {} logical cpus", std::env::consts::ARCH, std::thread::available_parallelism().map(|x| x.get()).unwrap_or(1)),
            "wall_s": started.elapsed().as_secs_f64(),
            "data_sources": [{"url": "https://governance.aave.com/t/v4-spoke-liquidation-parameters-impact-on-revenue/23993", "license": "public forum post, quoted for the missing-denominator observation only; no data used, all data synthetic and seeded"}],
        }
    });
    std::fs::write(out, serde_json::to_string_pretty(&v)? + "\n")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_for_seed() {
        assert_eq!(flips(&rates(7, 500)), flips(&rates(7, 500)));
    }

    #[test]
    fn flips_counts_swapped_pair() {
        let r = vec![[1.0, 2.0], [2.0, 1.0], [3.0, 3.0]];
        assert_eq!(flips(&r), 1);
    }

    #[test]
    fn pointer_resolves_in_written_json() {
        let p = std::env::temp_dir().join(format!("remainder-test-{}.json", std::process::id()));
        remaindercheck(7, 100, p.clone()).unwrap();
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
        assert!(v.pointer("/remainder/rank_flip_count").unwrap().is_u64());
        assert!(v.pointer("/provenance/started_at").unwrap().is_string());
    }
}
