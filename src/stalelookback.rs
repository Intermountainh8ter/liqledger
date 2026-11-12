//! `stalelookback`: liquidatable-flag flips when a landed liquidation is re-run against a pre-state a few blocks
//! stale. Parameters are fixed in preregistration_additions.md; no chain data is used.

use crate::defcheck::chrono_now;
use rand::{Rng, SeedableRng};
use std::path::PathBuf;

const MAX_LOOKBACK: usize = 8;
const STEP_SD: f64 = 0.003;

fn gauss(rng: &mut rand_pcg::Pcg64Mcg) -> f64 {
    let u1: f64 = 1.0 - rng.random::<f64>();
    let u2: f64 = rng.random::<f64>();
    (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
}

/// Flip counts for lookbacks 0..=MAX_LOOKBACK.
pub fn counts(seed: u64, n: usize) -> [usize; MAX_LOOKBACK + 1] {
    let mut rng = rand_pcg::Pcg64Mcg::seed_from_u64(seed);
    let mut out = [0usize; MAX_LOOKBACK + 1];
    for _ in 0..n {
        let mut hf: f64 = rng.random_range(0.95..1.0);
        for slot in out.iter_mut().skip(1) {
            hf += STEP_SD * gauss(&mut rng);
            if hf >= 1.0 {
                *slot += 1;
            }
        }
    }
    out
}

pub fn stalelookback(seed: u64, n: usize, out: PathBuf) -> anyhow::Result<()> {
    let started = std::time::Instant::now();
    let started_at = chrono_now();
    let c = counts(seed, n);
    let mut v = serde_json::json!({ "n_fixtures": n });
    for (k, x) in c.iter().enumerate() {
        v[format!("flag_flip_count_lookback_{k}")] = serde_json::json!(x);
    }
    v["provenance"] = serde_json::json!({
        "started_at": started_at,
        "seeds": [seed],
        "command": format!("cargo run --locked --release -- stalelookback --seed {seed} --n {n} --out results/111-stalelookback.json"),
        "hardware": format!("{} on {} logical cpus", std::env::consts::ARCH, std::thread::available_parallelism().map(|x| x.get()).unwrap_or(1)),
        "wall_s": started.elapsed().as_secs_f64(),
        "data_sources": [
            {"url": "synthetic: seeded generator inside the repository, no external data", "license": "MIT (repository licence)"}
        ],
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
    fn lookback_zero_is_zero_and_eight_exceeds_one() {
        let c = counts(7, 10000);
        assert_eq!(c[0], 0);
        assert!(c[8] > c[1]);
    }

    #[test]
    fn pointer_resolves_in_written_json() {
        let p = std::env::temp_dir().join(format!("stalelookback-test-{}.json", std::process::id()));
        stalelookback(7, 50, p.clone()).unwrap();
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
        assert!(v.pointer("/flag_flip_count_lookback_8").unwrap().is_u64());
        assert!(v.pointer("/provenance/started_at").unwrap().is_string());
    }
}
