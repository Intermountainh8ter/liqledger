//! `denomshare`: does the ranking of builders depend on the denominator of a "share of the bid".
//! Ranges and model are fixed in preregistration_additions.md; no chain data is used.

use crate::defcheck::chrono_now;
use rand::{Rng, SeedableRng};
use std::path::PathBuf;

pub const BUILDERS: usize = 5;

/// Per-builder ratios: bid over gross bonus, over post-execution value, over visible fee.
pub fn ratios(seed: u64, n: usize) -> Vec<[f64; 3]> {
    let mut rng = rand_pcg::Pcg64Mcg::seed_from_u64(seed);
    let gas: Vec<f64> = (0..BUILDERS).map(|_| rng.random_range(0.05..0.60)).collect();
    let vis: Vec<f64> = (0..BUILDERS).map(|_| rng.random_range(0.2..1.0)).collect();
    let mut sums = vec![[0.0f64; 4]; BUILDERS]; // gross, post, bid, visible
    for _ in 0..n {
        let b = rng.random_range(0..BUILDERS);
        let g: f64 = rng.random_range(1.0..100.0);
        let post = g * (1.0 - gas[b]);
        let bid = rng.random_range(0.5..1.0) * post;
        sums[b][0] += g;
        sums[b][1] += post;
        sums[b][2] += bid;
        sums[b][3] += vis[b] * bid;
    }
    sums.iter().map(|s| [s[2] / s[0], s[2] / s[1], s[2] / s[3]]).collect()
}

/// Pairs ordered differently under denominators `a` and `b` (indices into the ratio triple).
pub fn flips(r: &[[f64; 3]], a: usize, b: usize) -> usize {
    let mut c = 0;
    for i in 0..r.len() {
        for j in i + 1..r.len() {
            if (r[i][a] - r[j][a]) * (r[i][b] - r[j][b]) < 0.0 {
                c += 1;
            }
        }
    }
    c
}

pub fn denomshare(seed: u64, n: usize, out: PathBuf) -> anyhow::Result<()> {
    let started = std::time::Instant::now();
    let started_at = chrono_now();
    let r = ratios(seed, n);
    let v = serde_json::json!({
        "n_liquidations": n,
        "n_builders": BUILDERS,
        "n_pairs": BUILDERS * (BUILDERS - 1) / 2,
        "rank_flip_count": flips(&r, 0, 2),
        "flips_gross_vs_post": flips(&r, 0, 1),
        "flips_post_vs_visible": flips(&r, 1, 2),
        "builder_ratios": r.iter().map(|x| serde_json::json!({
            "bid_over_gross": x[0], "bid_over_post_exec": x[1], "bid_over_visible": x[2]})).collect::<Vec<_>>(),
        "provenance": {
            "started_at": started_at,
            "seeds": [seed],
            "command": format!("cargo run --locked --release -- denomshare --seed {seed} --n {n} --out results/71-denomshare.json"),
            "hardware": format!("{} on {} logical cpus", std::env::consts::ARCH, std::thread::available_parallelism().map(|x| x.get()).unwrap_or(1)),
            "wall_s": started.elapsed().as_secs_f64(),
            "data_sources": [{"url": "https://governance.aave.com/t/aave-v3-v4-liquidation-bot/25565", "license": "public forum post, quoted for denominator definitions only; no data used"}],
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
        assert_eq!(flips(&ratios(7, 500), 0, 2), flips(&ratios(7, 500), 0, 2));
    }

    #[test]
    fn flips_counts_swapped_pair() {
        let r = vec![[1.0, 0.0, 2.0], [2.0, 0.0, 1.0], [3.0, 0.0, 3.0]];
        assert_eq!(flips(&r, 0, 2), 1);
        assert_eq!(flips(&r, 0, 0), 0);
    }

    #[test]
    fn pointer_resolves_in_written_json() {
        let p = std::env::temp_dir().join(format!("denomshare-test-{}.json", std::process::id()));
        denomshare(7, 100, p.clone()).unwrap();
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
        assert!(v.pointer("/rank_flip_count").unwrap().is_u64());
        assert!(v.pointer("/provenance/started_at").unwrap().is_string());
    }
}
