//! `stalestate3`: stalestate2 with the oracle input recast as update timing (update lands one block after the local read). Parameters are
//! fixed in preregistration_additions.md; no chain data is used.

use crate::defcheck::chrono_now;
use rand::{Rng, SeedableRng};
use std::path::PathBuf;

const P: [f64; 3] = [0.4, 0.3, 0.1];
const SD: [f64; 3] = [0.012, 0.005, 0.008];

#[derive(Debug, PartialEq, Eq)]
pub struct Counts {
    pub no_omission: usize,
    pub oracle_timing_only: usize,
    pub tx_only: usize,
    pub balance_only: usize,
    pub multi: usize,
}

fn gauss(rng: &mut rand_pcg::Pcg64Mcg) -> f64 {
    let u1: f64 = 1.0 - rng.random::<f64>();
    let u2: f64 = rng.random::<f64>();
    (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
}

/// An input is "alone" for a row when its shift by itself flips the flag and no other single shift does; rows where
/// only a combination flips (or several singles flip) go to `multi`.
pub fn counts(seed: u64, n: usize) -> Counts {
    let mut rng = rand_pcg::Pcg64Mcg::seed_from_u64(seed);
    let mut c = Counts { no_omission: 0, oracle_timing_only: 0, tx_only: 0, balance_only: 0, multi: 0 };
    for _ in 0..n {
        let hf: f64 = rng.random_range(0.95..1.0);
        let mut s = [0.0f64; 3];
        let mut any = false;
        for i in 0..3 {
            let present = rng.random::<f64>() < P[i];
            any |= present;
            if present {
                s[i] = (SD[i] * gauss(&mut rng)).abs();
            }
        }
        if !any && hf >= 1.0 {
            c.no_omission += 1;
        }
        let f: Vec<bool> = s.iter().map(|x| hf + x >= 1.0).collect();
        match (f[0], f[1], f[2]) {
            (true, false, false) => c.oracle_timing_only += 1,
            (false, true, false) => c.tx_only += 1,
            (false, false, true) => c.balance_only += 1,
            (false, false, false) => {
                if hf + s[0] + s[1] + s[2] >= 1.0 {
                    c.multi += 1;
                }
            }
            _ => c.multi += 1,
        }
    }
    c
}

pub fn stalestate3(seed: u64, n: usize, out: PathBuf) -> anyhow::Result<()> {
    let started = std::time::Instant::now();
    let started_at = chrono_now();
    let c = counts(seed, n);
    let v = serde_json::json!({
        "n_fixtures": n,
        "stalestate3": {
            "oracle_timing_only_count": c.oracle_timing_only,
            "tx_only_count": c.tx_only,
            "balance_only_count": c.balance_only,
            "multi_input_count": c.multi,
            "no_omission_flip_count": c.no_omission,
        },
        "provenance": {
            "started_at": started_at,
            "seeds": [seed],
            "command": format!("cargo run --locked --release -- stalestate3 --seed {seed} --n {n} --out results/124-stalestate3.json"),
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
        assert_eq!(counts(7, 500), counts(7, 500));
    }

    #[test]
    fn oracle_timing_exceeds_tx_and_no_omission_is_zero() {
        let c = counts(7, 10000);
        assert_eq!(c.no_omission, 0);
        assert!(c.oracle_timing_only > c.tx_only);
    }

    #[test]
    fn pointer_resolves_in_written_json() {
        let p = std::env::temp_dir().join(format!("stalestate3-test-{}.json", std::process::id()));
        stalestate3(7, 50, p.clone()).unwrap();
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
        assert!(v.pointer("/stalestate3/tx_only_count").unwrap().is_u64());
        assert!(v.pointer("/provenance/started_at").unwrap().is_string());
    }
}
