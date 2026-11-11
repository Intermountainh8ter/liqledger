//! `replaygap`: toy re-execution that perturbs one input at a time and counts outcome mismatches.
//! Model is fixed in preregistration_additions.md; no chain data is used.

use crate::defcheck::chrono_now;
use rand::{Rng, SeedableRng};
use std::path::PathBuf;

/// (state_only, gas_only, ordering_only)
pub fn counts(seed: u64, n: usize) -> [usize; 3] {
    let mut rng = rand_pcg::Pcg64Mcg::seed_from_u64(seed);
    let mut c = [0usize; 3];
    for _ in 0..n {
        let hf: f64 = rng.random_range(0.95..1.0);
        let shock: f64 = rng.random_range(-0.03..0.03);
        let need: f64 = rng.random_range(150_000.0..450_000.0);
        let slack: f64 = rng.random_range(1.0..1.3);
        let factor: f64 = rng.random_range(0.8..1.2);
        let rival = rng.random::<f64>() < 0.1;
        let share: f64 = rng.random();
        let landed_ok = hf < 1.0;
        if (hf * (1.0 + shock) < 1.0) != landed_ok {
            c[0] += 1;
        }
        if need * slack * factor < need {
            c[1] += 1;
        }
        if rival && share > 0.5 {
            c[2] += 1;
        }
    }
    c
}

pub fn replaygap(seed: u64, n: usize, out: PathBuf) -> anyhow::Result<()> {
    let started = std::time::Instant::now();
    let started_at = chrono_now();
    let c = counts(seed, n);
    let v = serde_json::json!({
        "n_liquidations": n,
        "mismatch": {
            "by_input": {
                "state_only_count": c[0],
                "gas_only_count": c[1],
                "ordering_only_count": c[2],
            },
        },
        "provenance": {
            "started_at": started_at,
            "seeds": [seed],
            "command": format!("cargo run --locked --release -- replaygap --seed {seed} --n {n} --out results/84-replaygap.json"),
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
    fn counts_bounded_by_n() {
        let c = counts(3, 1000);
        assert!(c.iter().all(|&x| x <= 1000));
    }

    #[test]
    fn pointer_resolves_in_written_json() {
        let p = std::env::temp_dir().join(format!("replaygap-test-{}.json", std::process::id()));
        replaygap(7, 200, p.clone()).unwrap();
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
        assert!(v.pointer("/mismatch/by_input/ordering_only_count").unwrap().is_u64());
        assert!(v.pointer("/provenance/started_at").unwrap().is_string());
    }
}
