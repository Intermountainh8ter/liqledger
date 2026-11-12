//! `dustdeficit`: dust and deficit flags recomputed with floor versus ceil integer rounding near a dust
//! threshold. Parameters are fixed in preregistration_additions.md; no chain data is used.

use crate::defcheck::chrono_now;
use rand::{Rng, SeedableRng};
use std::path::PathBuf;

const DUST: i64 = 1000;

fn div(a: i64, b: i64, up: bool) -> i64 {
    if up { (a + b - 1) / b } else { a / b }
}

fn flags(debt: i64, coll: i64, up: bool) -> (bool, bool) {
    let repay = div(debt * 5000, 10000, up);
    let remaining = debt - repay;
    let seized = div(coll * repay, debt, up);
    let left = coll - seized;
    (remaining < DUST, remaining > 0 && left == 0)
}

/// Returns the number of rows where either flag differs between ceil and floor.
pub fn count(seed: u64, n: usize) -> usize {
    let mut rng = rand_pcg::Pcg64Mcg::seed_from_u64(seed);
    let mut dis = 0;
    for _ in 0..n {
        let debt: i64 = rng.random_range(1996..=2004);
        let coll = debt + rng.random_range(0..=2);
        if flags(debt, coll, true) != flags(debt, coll, false) {
            dis += 1;
        }
    }
    dis
}

pub fn dustdeficit(seed: u64, n: usize, out: PathBuf) -> anyhow::Result<()> {
    let started = std::time::Instant::now();
    let started_at = chrono_now();
    let d = count(seed, n);
    let v = serde_json::json!({
        "disagree_count": d,
        "n_fixtures": n,
        "provenance": {
            "started_at": started_at,
            "seeds": [seed],
            "command": format!("cargo run --locked -q --release -- dustdeficit --seed {seed} --n {n} --out results/106-dustdeficit.json"),
            "hardware": format!("{} on {} logical cpus", std::env::consts::ARCH, std::thread::available_parallelism().map(|x| x.get()).unwrap_or(1)),
            "wall_s": started.elapsed().as_secs_f64(),
            "data_sources": [
                {"url": "synthetic: seeded generator inside the repository, no external data", "license": "MIT (repository licence)"}
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
        assert_eq!(count(7, 200), count(7, 200));
    }

    #[test]
    fn nonzero_disagreements() {
        assert!(count(7, 1000) > 0);
    }

    #[test]
    fn pointer_resolves_in_written_json() {
        let p = std::env::temp_dir().join(format!("dustdeficit-test-{}.json", std::process::id()));
        dustdeficit(7, 50, p.clone()).unwrap();
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
        assert!(v.pointer("/disagree_count").unwrap().is_u64());
        assert!(v.pointer("/provenance/started_at").unwrap().is_string());
    }
}
