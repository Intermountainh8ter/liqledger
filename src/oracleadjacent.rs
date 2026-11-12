//! `oracleadjacent`: classify synthetic liquidations by transaction-index distance after a same-block oracle update,
//! separately from a mempool-visible flag. Parameters are fixed in preregistration_additions.md; no chain data is used.

use crate::defcheck::chrono_now;
use rand::{Rng, SeedableRng};
use std::path::PathBuf;

const BLOCK_TXS: i64 = 150;

#[derive(Debug, Default, PartialEq)]
pub struct Counts {
    pub adjacent: usize,
    pub oracle_triggered: usize,
    pub oracle_adjacent: usize,
    pub mempool_only: usize,
    pub mempool_only_adjacent: usize,
}

pub fn count(seed: u64, n: usize) -> Counts {
    let mut rng = rand_pcg::Pcg64Mcg::seed_from_u64(seed);
    let mut c = Counts::default();
    for _ in 0..n {
        let oracle = rng.random_bool(0.4);
        let mempool = rng.random_bool(0.3);
        let update: i64 = rng.random_range(0..BLOCK_TXS);
        let land: i64 = if oracle {
            update + rng.random_range(1..=4)
        } else if mempool && rng.random_bool(0.3) {
            update + rng.random_range(1..=3)
        } else {
            rng.random_range(0..BLOCK_TXS)
        };
        let d = land - update;
        let adj = (1..=3).contains(&d);
        c.adjacent += adj as usize;
        if oracle {
            c.oracle_triggered += 1;
            c.oracle_adjacent += adj as usize;
        } else if mempool {
            c.mempool_only += 1;
            c.mempool_only_adjacent += adj as usize;
        }
    }
    c
}

fn share(a: usize, b: usize) -> f64 {
    if b == 0 { 0.0 } else { a as f64 / b as f64 }
}

pub fn oracleadjacent(seed: u64, n: usize, out: PathBuf) -> anyhow::Result<()> {
    let started = std::time::Instant::now();
    let started_at = chrono_now();
    let c = count(seed, n);
    let v = serde_json::json!({
        "adjacent_count": c.adjacent,
        "oracle_triggered": c.oracle_triggered,
        "oracle_adjacent_share": share(c.oracle_adjacent, c.oracle_triggered),
        "mempool_only": c.mempool_only,
        "mempool_only_adjacent_share": share(c.mempool_only_adjacent, c.mempool_only),
        "n_fixtures": n,
        "provenance": {
            "started_at": started_at,
            "seeds": [seed],
            "command": format!("cargo run --locked --release -- oracleadjacent --seed {seed} --n {n} --out results/107-oracleadjacent.json"),
            "hardware": format!("{} on {} logical cpus", std::env::consts::ARCH, std::thread::available_parallelism().map(|x| x.get()).unwrap_or(1)),
            "wall_s": started.elapsed().as_secs_f64(),
            "data_sources": [
                {"url": "https://arxiv.org/abs/2606.03434", "license": "arXiv abstract page, cited for the ordering-rule definition only; no data used, all data synthetic and seeded"}
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
        assert_eq!(count(7, 500), count(7, 500));
    }

    #[test]
    fn oracle_rows_mostly_adjacent() {
        let c = count(7, 5000);
        assert!(c.oracle_adjacent * 10 > c.oracle_triggered * 6);
    }

    #[test]
    fn pointer_resolves_in_written_json() {
        let p = std::env::temp_dir().join(format!("oracleadjacent-test-{}.json", std::process::id()));
        oracleadjacent(7, 50, p.clone()).unwrap();
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
        assert!(v.pointer("/adjacent_count").unwrap().is_u64());
        assert!(v.pointer("/provenance/started_at").unwrap().is_string());
    }
}
