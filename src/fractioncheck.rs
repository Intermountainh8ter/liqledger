//! `fractioncheck`: how often does a fixed-fraction-of-debt repay leave the post-liquidation health
//! factor below a per-Spoke target. Ranges are fixed in preregistration_additions.md; no chain data is used.

use crate::bonuscheck::post_hf;
use crate::defcheck::chrono_now;
use rand::{Rng, SeedableRng};
use std::path::PathBuf;

pub const TARGETS: [f64; 3] = [1.035, 1.044, 1.24];
pub const TOLERANCE: f64 = 0.01;
pub const REPAY_FRACTION: f64 = 0.5;
pub const BONUS: f64 = 0.05;

/// True when the fixed-fraction repay ends more than the tolerance below `target`.
pub fn is_below(hf: f64, lt: f64, target: f64) -> bool {
    let c = hf / lt;
    post_hf(c, 1.0, lt, REPAY_FRACTION, BONUS) < target - TOLERANCE
}

pub fn count_below(seed: u64, n: usize) -> usize {
    let mut rng = rand_pcg::Pcg64Mcg::seed_from_u64(seed);
    (0..n)
        .filter(|_| {
            let hf: f64 = rng.random_range(0.90..0.99);
            let lt: f64 = rng.random_range(0.75..0.90);
            let target = TARGETS[rng.random_range(0..TARGETS.len())];
            is_below(hf, lt, target)
        })
        .count()
}

pub fn fractioncheck(seed: u64, n: usize, out: PathBuf) -> anyhow::Result<()> {
    let started = std::time::Instant::now();
    let started_at = chrono_now();
    let below = count_below(seed, n);
    let v = serde_json::json!({
        "summary": {
            "n_fixtures": n,
            "targets": TARGETS,
            "tolerance": TOLERANCE,
            "repay_fraction": REPAY_FRACTION,
            "fixed_bonus": BONUS,
            "fixed_fraction_below_target_count": below,
        },
        "provenance": {
            "started_at": started_at,
            "seeds": [seed],
            "command": format!("cargo run --locked --release -- fractioncheck --seed {seed} --n {n} --out results/59-fractioncheck.json"),
            "hardware": format!("{} on {} logical cpus", std::env::consts::ARCH, std::thread::available_parallelism().map(|x| x.get()).unwrap_or(1)),
            "wall_s": started.elapsed().as_secs_f64(),
            "data_sources": [{"url": "https://governance.aave.com/t/arfc-deploy-aave-v4-on-avalanche/25165", "license": "public forum post, quoted for rule definition only; no data used"}],
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
        assert_eq!(count_below(7, 200), count_below(7, 200));
    }

    #[test]
    fn high_target_always_missed() {
        assert!(is_below(0.99, 0.75, 1.24));
    }

    #[test]
    fn healthy_enough_not_below() {
        // hf 0.99, lt 0.75: post = 2*(0.99-0.39375) = 1.1925 >= 1.035-0.01
        assert!(!is_below(0.99, 0.75, 1.035));
    }

    #[test]
    fn pointer_resolves_in_written_json() {
        let p = std::env::temp_dir().join(format!("fractioncheck-test-{}.json", std::process::id()));
        fractioncheck(7, 50, p.clone()).unwrap();
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
        assert!(v.pointer("/summary/fixed_fraction_below_target_count").unwrap().is_u64());
        assert!(v.pointer("/provenance/started_at").unwrap().is_string());
    }
}
