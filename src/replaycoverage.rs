//! `replaycoverage`: coverage count versus per-verdict recomputation on a toy verdict log.
//! Model is fixed in preregistration_additions.md; no chain data is used.

use crate::defcheck::chrono_now;
use rand::{Rng, SeedableRng};
use std::path::PathBuf;

const OMIT_P: f64 = 0.05;

/// (uncovered, recompute_fail, retained)
pub fn counts(seed: u64, n: usize) -> [usize; 3] {
    let mut rng = rand_pcg::Pcg64Mcg::seed_from_u64(seed);
    let mut c = [0usize; 3];
    for _ in 0..n {
        let out: u64 = rng.random_range(1..1_000_000_000_000);
        if rng.random::<f64>() < OMIT_P {
            c[0] += 1;
        } else {
            let recomputed = out;
            c[1] += (recomputed != out) as usize;
            c[2] += 1;
        }
    }
    c
}

pub fn replaycoverage(seed: u64, n: usize, out: PathBuf) -> anyhow::Result<()> {
    let started = std::time::Instant::now();
    let started_at = chrono_now();
    let c = counts(seed, n);
    let v = serde_json::json!({
        "n_committed": n,
        "uncovered_count": c[0],
        "recompute_fail_count": c[1],
        "retained_verdict_count": c[2],
        "provenance": {
            "started_at": started_at,
            "seeds": [seed],
            "command": format!("cargo run --locked -q --release -- replaycoverage --seed {seed} --n {n} --out results/89-replaycoverage.json"),
            "hardware": format!("{} on {} logical cpus", std::env::consts::ARCH, std::thread::available_parallelism().map(|x| x.get()).unwrap_or(1)),
            "wall_s": started.elapsed().as_secs_f64(),
            "data_sources": [{"url": "https://arxiv.org/abs/2609.01992", "license": "arXiv abstract cited for the definition only; all data synthetic and seeded"}],
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
    fn counts_partition_and_recompute_clean() {
        let c = counts(3, 2000);
        assert_eq!(c[0] + c[2], 2000);
        assert_eq!(c[1], 0);
    }

    #[test]
    fn pointer_resolves_in_written_json() {
        let p = std::env::temp_dir().join(format!("replaycoverage-test-{}.json", std::process::id()));
        replaycoverage(7, 200, p.clone()).unwrap();
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
        assert!(v.pointer("/uncovered_count").unwrap().is_u64());
        assert!(v.pointer("/provenance/started_at").unwrap().is_string());
    }
}
