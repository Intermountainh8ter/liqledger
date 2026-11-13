//! `stalestate5`: a block-context field that differs between local simulation and the landed block. Parameters are
//! fixed in preregistration_additions.md; no chain data is used.

use crate::defcheck::chrono_now;
use rand::{Rng, SeedableRng};
use std::path::PathBuf;

const P_ENV: f64 = 0.20;
const SD: f64 = 0.01;

#[derive(Debug, PartialEq, Eq)]
pub struct Counts {
    pub environment: usize,
    pub identical_env_control: usize,
    pub residual: usize,
}

fn gauss(rng: &mut rand_pcg::Pcg64Mcg) -> f64 {
    let u1: f64 = 1.0 - rng.random::<f64>();
    let u2: f64 = rng.random::<f64>();
    (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
}

/// Returns counts for the given env_differs probability; a probability of zero is the no-env_differs control.
pub fn counts(seed: u64, n: usize, p_env: f64) -> Counts {
    let mut rng = rand_pcg::Pcg64Mcg::seed_from_u64(seed);
    let mut c = Counts { environment: 0, identical_env_control: 0, residual: 0 };
    for _ in 0..n {
        let hf: f64 = rng.random_range(0.95..1.0);
        let env_differs = rng.random::<f64>() < p_env;
        let shift = if env_differs { (SD * gauss(&mut rng)).abs() } else { 0.0 };
        let flip = hf + shift >= 1.0;
        if env_differs && flip {
            c.environment += 1;
        } else if flip {
            c.residual += 1;
        }
    }
    if p_env == 0.0 {
        c.identical_env_control = c.residual + c.environment;
    }
    c
}

pub fn stalestate5(seed: u64, n: usize, out: PathBuf) -> anyhow::Result<()> {
    let started = std::time::Instant::now();
    let started_at = chrono_now();
    let c = counts(seed, n, P_ENV);
    let ctl = counts(seed, n, 0.0);
    let v = serde_json::json!({
        "n_fixtures": n,
        "stalestate5": {
            "environment_divergence_count": c.environment,
            "unexplained_residual_count": c.residual,
            "identical_env_control_flip_count": ctl.identical_env_control,
        },
        "provenance": {
            "started_at": started_at,
            "seeds": [seed],
            "command": format!("cargo run --locked --release -- stalestate5 --seed {seed} --n {n} --out results/126-stalestate5.json"),
            "hardware": format!("{} on {} logical cpus", std::env::consts::ARCH, std::thread::available_parallelism().map(|x| x.get()).unwrap_or(1)),
            "wall_s": started.elapsed().as_secs_f64(),
            "data_sources": [
                {"url": "synthetic: seeded generator inside the repository, no external data", "license": "MIT (repository licence)"}
            ],
        },
    });
    std::fs::write(out, serde_json::to_string_pretty(&v)? + "\n")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_for_seed() {
        assert_eq!(counts(7, 500, P_ENV), counts(7, 500, P_ENV));
    }

    #[test]
    fn env_differs_flips_and_control_is_zero() {
        assert!(counts(7, 10000, P_ENV).environment > 0);
        assert_eq!(counts(7, 10000, 0.0).identical_env_control, 0);
    }

    #[test]
    fn pointer_resolves_in_written_json() {
        let p = std::env::temp_dir().join(format!("stalestate5-test-{}.json", std::process::id()));
        stalestate5(7, 50, p.clone()).unwrap();
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
        assert!(v.pointer("/stalestate5/environment_divergence_count").unwrap().is_u64());
        assert!(v.pointer("/provenance/started_at").unwrap().is_string());
    }
}
