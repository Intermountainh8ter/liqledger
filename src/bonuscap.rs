//! `bonuscap`: repay sizing that assumes an unsaturated bonus ramp when the true bonus saturates at a
//! ceiling. Ranges are fixed in preregistration_additions.md; no chain data is used.

use crate::bonuscheck::{post_hf, true_bonus, TARGET_HF, TOLERANCE};
use crate::defcheck::chrono_now;
use rand::{Rng, SeedableRng};
use std::path::PathBuf;

pub const BONUS_CAP: f64 = 0.07;
/// Health factor below which the capped bonus is saturated (ramp reaches the cap).
pub const CUTOFF_HF: f64 = 0.96;

pub fn capped_bonus(hf: f64) -> f64 {
    true_bonus(hf).min(BONUS_CAP)
}

/// True when sizing with the unsaturated ramp misses the target under the capped bonus.
pub fn is_miss(hf: f64, lt: f64) -> bool {
    let d = 1.0;
    let c = hf * d / lt;
    let assumed = true_bonus(hf);
    let x = ((TARGET_HF * d - c * lt) / (TARGET_HF - (1.0 + assumed) * lt)).min(d);
    let after = post_hf(c, d, lt, x, capped_bonus(hf));
    !((after - TARGET_HF).abs() <= TOLERANCE)
}

/// Returns (below-cutoff misses, above-cutoff misses, below-cutoff rows).
pub fn count(seed: u64, n: usize) -> (usize, usize, usize) {
    let mut rng = rand_pcg::Pcg64Mcg::seed_from_u64(seed);
    let (mut below, mut above, mut rows_below) = (0, 0, 0);
    for _ in 0..n {
        let hf: f64 = rng.random_range(0.90..0.99);
        let lt: f64 = rng.random_range(0.75..0.90);
        let low = hf < CUTOFF_HF;
        if low {
            rows_below += 1;
        }
        if is_miss(hf, lt) {
            if low {
                below += 1;
            } else {
                above += 1;
            }
        }
    }
    (below, above, rows_below)
}

pub fn bonuscap(seed: u64, n: usize, out: PathBuf) -> anyhow::Result<()> {
    let started = std::time::Instant::now();
    let started_at = chrono_now();
    let (below, above, rows_below) = count(seed, n);
    let v = serde_json::json!({
        "summary": {
            "n_fixtures": n,
            "target_health_factor": TARGET_HF,
            "tolerance": TOLERANCE,
            "bonus_cap": BONUS_CAP,
            "cutoff_health_factor": CUTOFF_HF,
            "rows_below_cutoff": rows_below,
            "below_target_count": below,
            "above_target_count": above,
        },
        "provenance": {
            "started_at": started_at,
            "seeds": [seed],
            "command": format!("cargo run --locked --release -- bonuscap --seed {seed} --n {n} --out results/91-bonuscap.json"),
            "hardware": format!("{} on {} logical cpus", std::env::consts::ARCH, std::thread::available_parallelism().map(|x| x.get()).unwrap_or(1)),
            "wall_s": started.elapsed().as_secs_f64(),
            "data_sources": [{"url": "https://aave.com/blog/aave-v4-liquidations", "license": "public blog post, quoted for rule definition only; no data used"}],
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
        assert_eq!(count(7, 200), count(7, 200));
    }

    #[test]
    fn cap_binds_only_below_cutoff() {
        assert!((capped_bonus(CUTOFF_HF) - BONUS_CAP).abs() < 1e-12);
        assert!((capped_bonus(0.98) - true_bonus(0.98)).abs() < 1e-12);
        assert!(capped_bonus(0.90) < true_bonus(0.90));
    }

    #[test]
    fn no_miss_above_cutoff() {
        assert!(!is_miss(0.98, 0.8));
    }

    #[test]
    fn pointer_resolves_in_written_json() {
        let p = std::env::temp_dir().join(format!("bonuscap-test-{}.json", std::process::id()));
        bonuscap(7, 50, p.clone()).unwrap();
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
        assert!(v.pointer("/summary/below_target_count").unwrap().is_u64());
        assert!(v.pointer("/provenance/started_at").unwrap().is_string());
    }
}
