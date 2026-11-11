//! `closefactor`: how often do three close-factor models repay different fractions of the same position.
//! Ranges and models are fixed in preregistration_additions.md; no chain data is used.

use crate::defcheck::chrono_now;
use rand::{Rng, SeedableRng};
use std::path::PathBuf;

pub const BONUS: f64 = 0.05;
pub const TARGET_HF: f64 = 1.05;
pub const CUT_HF: f64 = 0.95;
pub const THRESHOLD: f64 = 0.01;

pub fn fixed_fraction(_hf: f64, _lt: f64) -> f64 {
    0.5
}

pub fn piecewise_fraction(hf: f64, _lt: f64) -> f64 {
    if hf >= CUT_HF { 0.5 } else { 1.0 }
}

pub fn target_fraction(hf: f64, lt: f64) -> f64 {
    let c = hf / lt;
    if c < 1.0 / (1.0 + BONUS) {
        return 1.0;
    }
    ((TARGET_HF - c * lt) / (TARGET_HF - (1.0 + BONUS) * lt)).clamp(0.0, 1.0)
}

pub fn models_differ(hf: f64, lt: f64) -> bool {
    let f = [fixed_fraction(hf, lt), piecewise_fraction(hf, lt), target_fraction(hf, lt)];
    let max = f.iter().cloned().fold(f64::MIN, f64::max);
    let min = f.iter().cloned().fold(f64::MAX, f64::min);
    max - min > THRESHOLD
}

pub fn count_differ(seed: u64, n: usize) -> usize {
    let mut rng = rand_pcg::Pcg64Mcg::seed_from_u64(seed);
    (0..n)
        .filter(|_| {
            let hf: f64 = rng.random_range(0.90..0.99);
            let lt: f64 = rng.random_range(0.75..0.90);
            models_differ(hf, lt)
        })
        .count()
}

pub fn closefactor(seed: u64, n: usize, out: PathBuf) -> anyhow::Result<()> {
    let started = std::time::Instant::now();
    let started_at = chrono_now();
    let differ = count_differ(seed, n);
    let v = serde_json::json!({
        "disagreement": {
            "n_fixtures": n,
            "bonus": BONUS,
            "target_hf": TARGET_HF,
            "piecewise_cut_hf": CUT_HF,
            "threshold": THRESHOLD,
            "count_models_differ": differ,
        },
        "provenance": {
            "started_at": started_at,
            "seeds": [seed],
            "command": format!("cargo run --locked --release -- closefactor --seed {seed} --n {n} --out results/66-closefactor.json"),
            "hardware": format!("{} on {} logical cpus", std::env::consts::ARCH, std::thread::available_parallelism().map(|x| x.get()).unwrap_or(1)),
            "wall_s": started.elapsed().as_secs_f64(),
            "data_sources": [{"url": "https://aave.com/docs/aave-v4/liquidations", "license": "public vendor docs, quoted for rule definition only; no data used"}],
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
        assert_eq!(count_differ(7, 200), count_differ(7, 200));
    }

    #[test]
    fn below_cut_piecewise_is_full() {
        assert_eq!(piecewise_fraction(0.90, 0.8), 1.0);
        assert_eq!(piecewise_fraction(0.99, 0.8), 0.5);
    }

    #[test]
    fn bad_debt_threshold_is_full_repay() {
        // c = 0.90/0.80 = 1.125 >= 1/1.05, so not bad debt; hf 0.90 lt 0.95 gives c = 0.947 < 0.952
        assert_eq!(target_fraction(0.90, 0.95), 1.0);
        assert!(target_fraction(0.99, 0.75) < 1.0);
    }

    #[test]
    fn pointer_resolves_in_written_json() {
        let p = std::env::temp_dir().join(format!("closefactor-test-{}.json", std::process::id()));
        closefactor(7, 50, p.clone()).unwrap();
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
        assert!(v.pointer("/disagreement/count_models_differ").unwrap().is_u64());
        assert!(v.pointer("/provenance/started_at").unwrap().is_string());
    }
}
