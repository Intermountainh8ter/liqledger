//! `samebloc`: how much does the oracle-before-liquidation ordering rule change a same-block count.
//! Model is fixed in preregistration_additions.md; no chain data is used.

use crate::defcheck::chrono_now;
use rand::{Rng, SeedableRng};
use std::path::PathBuf;

pub const SLOTS: u32 = 150;

/// Counts of blocks matched under (any update, earlier update, latest update earlier).
pub fn counts(seed: u64, n: usize) -> [usize; 3] {
    let mut rng = rand_pcg::Pcg64Mcg::seed_from_u64(seed);
    let mut c = [0usize; 3];
    for _ in 0..n {
        let l = rng.random_range(0..SLOTS);
        let u: f64 = rng.random();
        let k = if u < 0.4 { 0 } else if u < 0.8 { 1 } else { 2 };
        let mut ups: Vec<u32> = Vec::new();
        while ups.len() < k {
            let s = rng.random_range(0..SLOTS);
            if s != l && !ups.contains(&s) {
                ups.push(s);
            }
        }
        if ups.is_empty() {
            continue;
        }
        c[0] += 1;
        if ups.iter().any(|&s| s < l) {
            c[1] += 1;
        }
        if ups.iter().all(|&s| s < l) {
            c[2] += 1;
        }
    }
    c
}

pub fn samebloc(seed: u64, n: usize, out: PathBuf) -> anyhow::Result<()> {
    let started = std::time::Instant::now();
    let started_at = chrono_now();
    let c = counts(seed, n);
    let v = serde_json::json!({
        "n_blocks": n,
        "slots_per_block": SLOTS,
        "count_any": c[0],
        "count_earlier": c[1],
        "count_latest_earlier": c[2],
        "spread_any_vs_earlier": c[0] as i64 - c[1] as i64,
        "spread_any_vs_latest": c[0] as i64 - c[2] as i64,
        "provenance": {
            "started_at": started_at,
            "seeds": [seed],
            "command": format!("cargo run --locked --release -- samebloc --seed {seed} --n {n} --out results/76-samebloc.json"),
            "hardware": format!("{} on {} logical cpus", std::env::consts::ARCH, std::thread::available_parallelism().map(|x| x.get()).unwrap_or(1)),
            "wall_s": started.elapsed().as_secs_f64(),
            "data_sources": [{"url": "https://arxiv.org/abs/2606.03434", "license": "arXiv abstract page, cited for the ordering-rule definition only; no data used, all data synthetic and seeded"}],
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
    fn rules_are_nested() {
        let c = counts(3, 2000);
        assert!(c[2] <= c[1] && c[1] <= c[0]);
    }

    #[test]
    fn pointer_resolves_in_written_json() {
        let p = std::env::temp_dir().join(format!("samebloc-test-{}.json", std::process::id()));
        samebloc(7, 200, p.clone()).unwrap();
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
        assert!(v.pointer("/spread_any_vs_earlier").unwrap().is_i64());
        assert!(v.pointer("/provenance/started_at").unwrap().is_string());
    }
}
