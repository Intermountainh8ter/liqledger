//! `bonuscheck`: how often does repay sizing with a fixed bonus miss the target health factor
//! when the true bonus depends on health factor. Ranges are fixed in preregistration_additions.md;
//! no chain data is used.

use crate::defcheck::chrono_now;
use rand::{Rng, SeedableRng};
use std::path::PathBuf;

pub const TARGET_HF: f64 = 1.05;
pub const TOLERANCE: f64 = 0.01;
pub const FIXED_BONUS: f64 = 0.05;

pub fn true_bonus(hf: f64) -> f64 {
    0.05 + 0.05 * (1.0 - hf) / 0.10
}

/// Post-liquidation health factor for a repay of `x` with the given bonus (debt `d`, collateral `c`).
pub fn post_hf(c: f64, d: f64, lt: f64, x: f64, bonus: f64) -> f64 {
    if x >= d {
        return f64::INFINITY;
    }
    (c - x * (1.0 + bonus)) * lt / (d - x)
}

/// Returns true when the fixed-bonus sized repay misses the target by more than the tolerance.
pub fn is_miss(hf: f64, lt: f64) -> bool {
    let d = 1.0;
    let c = hf * d / lt;
    let x = ((TARGET_HF * d - c * lt) / (TARGET_HF - (1.0 + FIXED_BONUS) * lt)).min(d);
    let after = post_hf(c, d, lt, x, true_bonus(hf));
    !((after - TARGET_HF).abs() <= TOLERANCE)
}

pub fn count_misses(seed: u64, n: usize) -> usize {
    let mut rng = rand_pcg::Pcg64Mcg::seed_from_u64(seed);
    (0..n)
        .filter(|_| {
            let hf: f64 = rng.random_range(0.90..0.99);
            let lt: f64 = rng.random_range(0.75..0.90);
            is_miss(hf, lt)
        })
        .count()
}

pub fn bonuscheck(seed: u64, n: usize, out: PathBuf) -> anyhow::Result<()> {
    let started = std::time::Instant::now();
    let started_at = chrono_now();
    let misses = count_misses(seed, n);
    let v = serde_json::json!({
        "summary": {
            "n_fixtures": n,
            "target_health_factor": TARGET_HF,
            "tolerance": TOLERANCE,
            "fixed_bonus": FIXED_BONUS,
            "fixed_bonus_target_miss_count": misses,
        },
        "provenance": {
            "started_at": started_at,
            "seeds": [seed],
            "command": format!("cargo run --locked --release -- bonuscheck --seed {seed} --n {n} --out results/58-bonuscheck.json"),
            "hardware": format!("{} on {} logical cpus", std::env::consts::ARCH, std::thread::available_parallelism().map(|x| x.get()).unwrap_or(1)),
            "wall_s": started.elapsed().as_secs_f64(),
            "data_sources": [{"url": "https://governance.aave.com/t/overview-of-aave-v4-s-liquidation-engine/23531", "license": "public forum post, quoted for rule definition only; no data used"}],
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
        assert_eq!(count_misses(7, 200), count_misses(7, 200));
    }

    #[test]
    fn bonus_matches_fixed_at_hf_one() {
        assert!((true_bonus(1.0) - FIXED_BONUS).abs() < 1e-12);
        assert!(true_bonus(0.9) > true_bonus(0.95));
    }

    #[test]
    fn exact_when_bonus_equals_fixed() {
        let (hf, lt) = (0.95, 0.8);
        let c = hf / lt;
        let x = (TARGET_HF - c * lt) / (TARGET_HF - (1.0 + FIXED_BONUS) * lt);
        assert!((post_hf(c, 1.0, lt, x, FIXED_BONUS) - TARGET_HF).abs() < 1e-9);
    }

    #[test]
    fn pointer_resolves_in_written_json() {
        let p = std::env::temp_dir().join(format!("bonuscheck-test-{}.json", std::process::id()));
        bonuscheck(7, 50, p.clone()).unwrap();
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
        assert!(v.pointer("/summary/fixed_bonus_target_miss_count").unwrap().is_u64());
        assert!(v.pointer("/provenance/started_at").unwrap().is_string());
    }
}
