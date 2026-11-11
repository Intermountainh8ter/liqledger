//! `bonusscale`: a ledger that applies one shared bonus scale factor to profiles whose true scale
//! factors differ. Parameters are fixed in preregistration_additions.md; no chain data is used.

use crate::defcheck::chrono_now;
use rand::{Rng, SeedableRng};
use std::path::PathBuf;

pub const SHARED_SCALE: i64 = 60;
/// (base bps, scale bps per 1% below hf 1.0, max bps)
pub const VOLATILE: (i64, i64, i64) = (500, 100, 1000);
pub const CORRELATED: (i64, i64, i64) = (200, 25, 400);

pub fn bonus_bps(hf_bps: i64, p: (i64, i64, i64), scale: i64) -> i64 {
    (p.0 + scale * (10000 - hf_bps) / 100).min(p.2)
}

/// Returns (volatile rows, correlated rows, volatile misprices, correlated misprices).
pub fn count(seed: u64, n: usize, truth_scales: (i64, i64)) -> (usize, usize, usize, usize) {
    let mut rng = rand_pcg::Pcg64Mcg::seed_from_u64(seed);
    let (mut rv, mut rc, mut mv, mut mc) = (0, 0, 0, 0);
    for _ in 0..n {
        let hf: i64 = rng.random_range(9000..10000);
        let volatile: bool = rng.random_bool(0.5);
        if volatile {
            rv += 1;
            if bonus_bps(hf, VOLATILE, truth_scales.0) != bonus_bps(hf, VOLATILE, SHARED_SCALE) {
                mv += 1;
            }
        } else {
            rc += 1;
            if bonus_bps(hf, CORRELATED, truth_scales.1) != bonus_bps(hf, CORRELATED, SHARED_SCALE) {
                mc += 1;
            }
        }
    }
    (rv, rc, mv, mc)
}

pub fn bonusscale(seed: u64, n: usize, out: PathBuf) -> anyhow::Result<()> {
    let started = std::time::Instant::now();
    let started_at = chrono_now();
    let (rv, rc, mv, mc) = count(seed, n, (VOLATILE.1, CORRELATED.1));
    let (_, _, cv, cc) = count(seed, n, (SHARED_SCALE, SHARED_SCALE));
    let v = serde_json::json!({
        "misprice_count": mv + mc,
        "volatile": {"rows": rv, "misprice_count": mv, "true_scale": VOLATILE.1},
        "correlated": {"rows": rc, "misprice_count": mc, "true_scale": CORRELATED.1},
        "shared_scale": SHARED_SCALE,
        "n_fixtures": n,
        "control_misprice_count": cv + cc,
        "provenance": {
            "started_at": started_at,
            "seeds": [seed],
            "command": format!("cargo run --locked -q --release -- bonusscale --seed {seed} --out results/92-bonusscale.json"),
            "hardware": format!("{} on {} logical cpus", std::env::consts::ARCH, std::thread::available_parallelism().map(|x| x.get()).unwrap_or(1)),
            "wall_s": started.elapsed().as_secs_f64(),
            "data_sources": [
                {"url": "synthetic: seeded generator inside the repository, no external data", "license": "MIT (repository licence)"},
                {"url": "https://governance.aave.com/t/arfc-deploy-aave-v4-on-avalanche/25165", "license": "public governance forum post, parameter names only, no data used"}
            ],
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
        assert_eq!(count(7, 200, (100, 25)), count(7, 200, (100, 25)));
    }

    #[test]
    fn same_scale_is_zero() {
        let (_, _, a, b) = count(7, 500, (SHARED_SCALE, SHARED_SCALE));
        assert_eq!(a + b, 0);
    }

    #[test]
    fn differing_scales_misprice_both() {
        let (_, _, a, b) = count(7, 1000, (VOLATILE.1, CORRELATED.1));
        assert!(a > 0 && b > 0);
    }

    #[test]
    fn pointer_resolves_in_written_json() {
        let p = std::env::temp_dir().join(format!("bonusscale-test-{}.json", std::process::id()));
        bonusscale(7, 50, p.clone()).unwrap();
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
        assert!(v.pointer("/misprice_count").unwrap().is_u64());
        assert!(v.pointer("/provenance/started_at").unwrap().is_string());
    }
}
