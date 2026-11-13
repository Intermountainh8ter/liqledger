//! `stalestate4`: two in-transaction price reads that the landed path splits across an oracle update. Parameters are
//! fixed in preregistration_additions.md; no chain data is used.

use crate::defcheck::chrono_now;
use rand::{Rng, SeedableRng};
use std::path::PathBuf;

const P_SPLIT: f64 = 0.25;
const SD: f64 = 0.01;

#[derive(Debug, PartialEq, Eq)]
pub struct Counts {
    pub multiread: usize,
    pub no_split_control: usize,
    pub residual: usize,
}

fn gauss(rng: &mut rand_pcg::Pcg64Mcg) -> f64 {
    let u1: f64 = 1.0 - rng.random::<f64>();
    let u2: f64 = rng.random::<f64>();
    (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
}

/// Returns counts for the given split probability; a probability of zero is the no-split control.
pub fn counts(seed: u64, n: usize, p_split: f64) -> Counts {
    let mut rng = rand_pcg::Pcg64Mcg::seed_from_u64(seed);
    let mut c = Counts { multiread: 0, no_split_control: 0, residual: 0 };
    for _ in 0..n {
        let hf: f64 = rng.random_range(0.95..1.0);
        let split = rng.random::<f64>() < p_split;
        let shift = if split { (SD * gauss(&mut rng)).abs() } else { 0.0 };
        let flip = hf + shift >= 1.0;
        if split && flip {
            c.multiread += 1;
        } else if flip {
            c.residual += 1;
        }
    }
    if p_split == 0.0 {
        c.no_split_control = c.residual + c.multiread;
    }
    c
}

pub fn stalestate4(seed: u64, n: usize, out: PathBuf) -> anyhow::Result<()> {
    let started = std::time::Instant::now();
    let started_at = chrono_now();
    let c = counts(seed, n, P_SPLIT);
    let ctl = counts(seed, n, 0.0);
    let v = serde_json::json!({
        "n_fixtures": n,
        "stalestate4": {
            "multiread_divergence_count": c.multiread,
            "unexplained_residual_count": c.residual,
            "no_split_control_flip_count": ctl.no_split_control,
        },
        "provenance": {
            "started_at": started_at,
            "seeds": [seed],
            "command": format!("cargo run --locked --release -- stalestate4 --seed {seed} --n {n} --out results/125-stalestate4.json"),
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
        assert_eq!(counts(7, 500, P_SPLIT), counts(7, 500, P_SPLIT));
    }

    #[test]
    fn split_flips_and_control_is_zero() {
        assert!(counts(7, 10000, P_SPLIT).multiread > 0);
        assert_eq!(counts(7, 10000, 0.0).no_split_control, 0);
    }

    #[test]
    fn pointer_resolves_in_written_json() {
        let p = std::env::temp_dir().join(format!("stalestate4-test-{}.json", std::process::id()));
        stalestate4(7, 50, p.clone()).unwrap();
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
        assert!(v.pointer("/stalestate4/multiread_divergence_count").unwrap().is_u64());
        assert!(v.pointer("/provenance/started_at").unwrap().is_string());
    }
}
