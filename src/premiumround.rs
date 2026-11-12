//! `premiumround`: a ledger that adds up to 2 wei of rounded premium debt per accrual, compared with the
//! exact integer recompute near the liquidation boundary. Parameters are fixed in
//! preregistration_additions.md; no chain data is used.

use crate::defcheck::chrono_now;
use rand::{Rng, SeedableRng};
use std::path::PathBuf;

/// Returns (band disagreements, control disagreements).
pub fn count(seed: u64, n: usize) -> (usize, usize) {
    let mut rng = rand_pcg::Pcg64Mcg::seed_from_u64(seed);
    let (mut band, mut control) = (0, 0);
    for _ in 0..n {
        let offset: i64 = rng.random_range(-4..=4);
        let k: i64 = rng.random_range(1..=3);
        let add: i64 = (0..k).map(|_| rng.random_range(0..=2)).sum();
        if (offset > 0) != (offset + add > 0) {
            band += 1;
        }
        let u: i64 = rng.random_range(0..=3);
        let offset = -(2 * k + 1 + u);
        let add: i64 = (0..k).map(|_| rng.random_range(0..=2)).sum();
        if (offset > 0) != (offset + add > 0) {
            control += 1;
        }
    }
    (band, control)
}

pub fn premiumround(seed: u64, n: usize, out: PathBuf) -> anyhow::Result<()> {
    let started = std::time::Instant::now();
    let started_at = chrono_now();
    let (band, control) = count(seed, n);
    let v = serde_json::json!({
        "disagreements": band,
        "control_disagreements": control,
        "n_fixtures": n,
        "provenance": {
            "started_at": started_at,
            "seeds": [seed],
            "command": format!("cargo run --locked -q --release -- premiumround --seed {seed} --out results/102-premiumround.json"),
            "hardware": format!("{} on {} logical cpus", std::env::consts::ARCH, std::thread::available_parallelism().map(|x| x.get()).unwrap_or(1)),
            "wall_s": started.elapsed().as_secs_f64(),
            "data_sources": [
                {"url": "synthetic: seeded generator inside the repository, no external data", "license": "MIT (repository licence)"},
                {"url": "https://www.chainsecurity.com/security-audit/aave-v4", "license": "public audit page, finding description only, no data used"}
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
    fn band_flips_and_control_does_not() {
        let (b, c) = count(7, 1000);
        assert!(b > 0);
        assert_eq!(c, 0);
    }

    #[test]
    fn pointer_resolves_in_written_json() {
        let p = std::env::temp_dir().join(format!("premiumround-test-{}.json", std::process::id()));
        premiumround(7, 50, p.clone()).unwrap();
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
        assert!(v.pointer("/disagreements").unwrap().is_u64());
        assert!(v.pointer("/provenance/started_at").unwrap().is_string());
    }
}
