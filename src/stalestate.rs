//! `stalestate`: attribute liquidatable-flag flips in a stale pre-state to the omitted input that causes them, by
//! single-input counterfactuals. Parameters are fixed in preregistration_additions.md; no chain data is used.

use crate::defcheck::chrono_now;
use rand::{Rng, SeedableRng};
use std::path::PathBuf;

const ORACLE_P: f64 = 0.6;
const ORACLE_SD: f64 = 0.01;
const TX_P: f64 = 0.3;
const TX_SD: f64 = 0.005;

#[derive(Debug, PartialEq, Eq)]
pub struct Counts {
    pub no_omission: usize,
    pub oracle_only: usize,
    pub tx_only: usize,
    pub both: usize,
    pub oracle_present: usize,
    pub tx_present: usize,
}

fn gauss(rng: &mut rand_pcg::Pcg64Mcg) -> f64 {
    let u1: f64 = 1.0 - rng.random::<f64>();
    let u2: f64 = rng.random::<f64>();
    (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
}

/// Flag flips (stale health factor at or above 1) under each single-input counterfactual. `oracle_only` counts rows
/// where omitting just the oracle update flips the flag; `tx_only` likewise for the earlier transaction; `both`
/// where only omitting both flips it; `no_omission` is the flip count with nothing omitted.
pub fn counts(seed: u64, n: usize) -> Counts {
    let mut rng = rand_pcg::Pcg64Mcg::seed_from_u64(seed);
    let mut c = Counts { no_omission: 0, oracle_only: 0, tx_only: 0, both: 0, oracle_present: 0, tx_present: 0 };
    for _ in 0..n {
        let hf: f64 = rng.random_range(0.95..1.0);
        let has_o = rng.random::<f64>() < ORACLE_P;
        let has_t = rng.random::<f64>() < TX_P;
        let so = if has_o { (ORACLE_SD * gauss(&mut rng)).abs() } else { 0.0 };
        let st = if has_t { (TX_SD * gauss(&mut rng)).abs() } else { 0.0 };
        c.oracle_present += has_o as usize;
        c.tx_present += has_t as usize;
        let flip = |s: f64| hf + s >= 1.0;
        // The landed state has every input applied, so hf is the landed value; omitting inputs raises it.
        if !has_o && !has_t && flip(0.0) {
            c.no_omission += 1;
        }
        match (flip(so), flip(st), flip(so + st)) {
            (true, false, _) => c.oracle_only += 1,
            (false, true, _) => c.tx_only += 1,
            (true, true, _) => c.both += 1,
            (false, false, true) => c.both += 1,
            _ => {}
        }
    }
    c
}

pub fn stalestate(seed: u64, n: usize, out: PathBuf) -> anyhow::Result<()> {
    let started = std::time::Instant::now();
    let started_at = chrono_now();
    let c = counts(seed, n);
    let v = serde_json::json!({
        "n_fixtures": n,
        "stalestate": {
            "oracle_only_count": c.oracle_only,
            "tx_only_count": c.tx_only,
            "both_count": c.both,
            "no_omission_flip_count": c.no_omission,
            "oracle_present_rows": c.oracle_present,
            "tx_present_rows": c.tx_present,
        },
        "provenance": {
            "started_at": started_at,
            "seeds": [seed],
            "command": format!("cargo run --locked --release -- stalestate --seed {seed} --n {n} --out results/121-stalestate.json"),
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
    fn oracle_exceeds_tx_and_no_omission_is_zero() {
        let c = counts(7, 10000);
        assert_eq!(c.no_omission, 0);
        assert!(c.oracle_only > c.tx_only);
    }

    #[test]
    fn pointer_resolves_in_written_json() {
        let p = std::env::temp_dir().join(format!("stalestate-test-{}.json", std::process::id()));
        stalestate(7, 50, p.clone()).unwrap();
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
        assert!(v.pointer("/stalestate/oracle_only_count").unwrap().is_u64());
        assert!(v.pointer("/provenance/started_at").unwrap().is_string());
    }
}
