//! `replayverify`: exact integer verifier versus tolerance grader on toy replays.
//! Model is fixed in preregistration_additions.md; no chain data is used.

use crate::defcheck::chrono_now;
use rand::{Rng, SeedableRng};
use std::path::PathBuf;

const TOLERANCE: f64 = 1e-6;

/// (exact_fail, tolerance_fail, flips)
pub fn counts(seed: u64, n: usize) -> [usize; 3] {
    let mut rng = rand_pcg::Pcg64Mcg::seed_from_u64(seed);
    let mut c = [0usize; 3];
    for _ in 0..n {
        let amount: u64 = rng.random_range(1_000_000..1_000_000_000_000);
        let perturbed = rng.random::<f64>() < 0.3;
        let u: f64 = rng.random_range(-9.0..-3.0);
        let diff = if perturbed { (amount as f64 * 10f64.powf(u)).round() as u64 } else { 0 };
        let exact_fail = diff > 0;
        let tol_fail = diff as f64 / amount as f64 > TOLERANCE;
        c[0] += exact_fail as usize;
        c[1] += tol_fail as usize;
        c[2] += (exact_fail && !tol_fail) as usize;
    }
    c
}

pub fn replayverify(seed: u64, n: usize, out: PathBuf) -> anyhow::Result<()> {
    let started = std::time::Instant::now();
    let started_at = chrono_now();
    let c = counts(seed, n);
    let v = serde_json::json!({
        "n_replays": n,
        "exact_fail_count": c[0],
        "tolerance_fail_count": c[1],
        "verdict_flip_count": c[2],
        "provenance": {
            "started_at": started_at,
            "seeds": [seed],
            "command": format!("cargo run --locked -q --release -- replayverify --seed {seed} --n {n} --out results/88-replayverify.json"),
            "hardware": format!("{} on {} logical cpus", std::env::consts::ARCH, std::thread::available_parallelism().map(|x| x.get()).unwrap_or(1)),
            "wall_s": started.elapsed().as_secs_f64(),
            "data_sources": [{"url": "https://docs.base.org/base-chain/network-information/transaction-ordering.md", "license": "official docs, cited for definitions only; all data synthetic and seeded"}],
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
        assert_eq!(counts(7, 500), counts(7, 500));
    }

    #[test]
    fn flips_bounded_by_exact_fails() {
        let c = counts(3, 2000);
        assert!(c[2] <= c[0] && c[0] <= 2000 && c[1] <= c[0]);
        assert_eq!(c[0] - c[1], c[2]);
    }

    #[test]
    fn pointer_resolves_in_written_json() {
        let p = std::env::temp_dir().join(format!("replayverify-test-{}.json", std::process::id()));
        replayverify(7, 200, p.clone()).unwrap();
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
        assert!(v.pointer("/verdict_flip_count").unwrap().is_u64());
        assert!(v.pointer("/provenance/started_at").unwrap().is_string());
    }
}
