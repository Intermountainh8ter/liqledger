//! `dustcheck`: how often does a remaining-debt dust rule change the repaid amount versus a plain
//! target-health-factor repayment, on synthetic fixtures. Ranges are assumptions fixed in
//! preregistration_additions.md; no chain data is used.

use crate::defcheck::chrono_now;
use rand::{Rng, SeedableRng};
use std::path::PathBuf;

pub const DUST_USD: f64 = 1000.0;

pub struct Fixture {
    pub debt: f64,
    pub plain_repay: f64,
    pub dust_repay: f64,
}

pub fn fixtures(seed: u64, n: usize) -> Vec<Fixture> {
    let mut rng = rand_pcg::Pcg64Mcg::seed_from_u64(seed);
    (0..n)
        .map(|_| {
            let debt: f64 = rng.random_range(500.0..10000.0);
            let frac: f64 = rng.random_range(0.05..0.6);
            let plain_repay = debt * frac;
            let dust_repay = if debt - plain_repay < DUST_USD { debt } else { plain_repay };
            Fixture { debt, plain_repay, dust_repay }
        })
        .collect()
}

pub fn diverges(f: &Fixture) -> bool {
    f.dust_repay != f.plain_repay
}

pub fn dustcheck(seed: u64, n: usize, out: PathBuf) -> anyhow::Result<()> {
    let started = std::time::Instant::now();
    let started_at = chrono_now();
    let fx = fixtures(seed, n);
    let count = fx.iter().filter(|f| diverges(f)).count();
    let full = fx.iter().filter(|f| diverges(f) && f.dust_repay == f.debt).count();
    let below = fx.iter().filter(|f| f.debt < DUST_USD).count();
    let v = serde_json::json!({
        "summary": {
            "n_fixtures": n,
            "dust_threshold_usd": DUST_USD,
            "dust_repay_divergence_count": count,
            "divergent_full_repay_count": full,
            "fixtures_with_debt_below_threshold_count": below,
        },
        "provenance": {
            "started_at": started_at,
            "seeds": [seed],
            "command": format!("cargo run --locked --release -- dustcheck --seed {seed} --n {n} --out results/52-dustcheck.json"),
            "hardware": format!("{} on {} logical cpus", std::env::consts::ARCH, std::thread::available_parallelism().map(|x| x.get()).unwrap_or(1)),
            "wall_s": started.elapsed().as_secs_f64(),
            "data_sources": [{"url": "https://aave.com/docs/aave-v4/positions/liquidations", "license": "official documentation, quoted for rule definitions only; no data used"}],
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
        let a = fixtures(7, 200).iter().filter(|f| diverges(f)).count();
        let b = fixtures(7, 200).iter().filter(|f| diverges(f)).count();
        assert_eq!(a, b);
    }

    #[test]
    fn divergent_fixtures_leave_dust_under_plain_rule() {
        for f in fixtures(3, 500).iter().filter(|f| diverges(f)) {
            assert!(f.debt - f.plain_repay < DUST_USD);
        }
    }

    #[test]
    fn pointer_resolves_in_written_json() {
        let p = std::env::temp_dir().join(format!("dustcheck-test-{}.json", std::process::id()));
        dustcheck(7, 50, p.clone()).unwrap();
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
        assert!(v.pointer("/summary/dust_repay_divergence_count").unwrap().is_u64());
        assert!(v.pointer("/provenance/started_at").unwrap().is_string());
    }
}
