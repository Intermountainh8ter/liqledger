//! `refundcheck`: how often do gross-of-refund and net-of-refund shares disagree on synthetic fixtures.
//! The channel and refund mix is an assumption fixed in preregistration_additions.md; no chain data is used.

use crate::defcheck::{chrono_now, THRESHOLD};
use rand::{Rng, SeedableRng};
use std::path::PathBuf;

pub struct Fixture {
    pub gross: f64,
    pub net: f64,
    pub refunded: bool,
    pub visible: bool,
}

pub fn fixtures(seed: u64, n: usize) -> Vec<Fixture> {
    let mut rng = rand_pcg::Pcg64Mcg::seed_from_u64(seed);
    (0..n)
        .map(|_| {
            let prio: f64 = rng.random_range(0.0..0.5);
            let cb = if rng.random_bool(0.5) { rng.random_range(0.0..0.6) } else { 0.0 };
            let tip = if rng.random_bool(0.1) { rng.random_range(0.0..0.2) } else { 0.0 };
            let gross = prio + cb + tip;
            let refunded = rng.random_bool(0.3);
            let (frac, visible) = if refunded {
                (rng.random_range(0.0..0.5), rng.random_bool(0.2))
            } else {
                (0.0, false)
            };
            Fixture { gross, net: gross * (1.0 - frac), refunded, visible }
        })
        .collect()
}

pub fn disagrees(f: &Fixture) -> bool {
    (f.gross - f.net).abs() > THRESHOLD
}

pub fn refundcheck(seed: u64, n: usize, out: PathBuf) -> anyhow::Result<()> {
    let started = std::time::Instant::now();
    let started_at = chrono_now();
    let fx = fixtures(seed, n);
    let total = fx.iter().filter(|f| disagrees(f)).count();
    let visible = fx.iter().filter(|f| disagrees(f) && f.visible).count();
    let hidden = fx.iter().filter(|f| disagrees(f) && !f.visible).count();
    let refunded = fx.iter().filter(|f| f.refunded).count();
    let v = serde_json::json!({
        "summary": {
            "n_fixtures": n,
            "threshold": THRESHOLD,
            "refunded_fixture_count": refunded,
            "refund_gross_net_disagreement_count": total,
            "disagreement_visible_refund_count": visible,
            "disagreement_not_visible_refund_count": hidden,
        },
        "provenance": {
            "started_at": started_at,
            "seeds": [seed],
            "command": format!("cargo run --locked --release -- refundcheck --seed {seed} --n {n} --out results/51-refundcheck.json"),
            "hardware": format!("{} on {} logical cpus", std::env::consts::ARCH, std::thread::available_parallelism().map(|x| x.get()).unwrap_or(1)),
            "wall_s": started.elapsed().as_secs_f64(),
            "data_sources": [{"url": "https://greenfieldcapital.com/2026/08/06/the-state-of-ethereum-block-building-since-2024-part-i-the-demand-side-orderflow-and-the-priority-fee-landscape-2/", "license": "public blog post, quoted for definitions only; no data used"}],
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
        let a = fixtures(7, 200).iter().filter(|f| disagrees(f)).count();
        let b = fixtures(7, 200).iter().filter(|f| disagrees(f)).count();
        assert_eq!(a, b);
    }

    #[test]
    fn unrefunded_never_disagree() {
        assert!(fixtures(3, 300).iter().filter(|f| !f.refunded).all(|f| !disagrees(f)));
    }

    #[test]
    fn pointer_resolves_in_written_json() {
        let p = std::env::temp_dir().join(format!("refundcheck-test-{}.json", std::process::id()));
        refundcheck(7, 50, p.clone()).unwrap();
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
        assert!(v.pointer("/summary/refund_gross_net_disagreement_count").unwrap().is_u64());
        assert!(v.pointer("/provenance/started_at").unwrap().is_string());
    }
}
