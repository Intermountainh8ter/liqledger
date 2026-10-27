//! `fallbackcheck`: how many synthetic liquidations a public-only adjacency rule attributes to the wrong
//! update path at a short and a long fallback delay. The mix is an assumption fixed in
//! preregistration_additions.md; no chain data is used.

use crate::defcheck::chrono_now;
use rand::{Rng, SeedableRng};
use std::path::PathBuf;

/// Mean unrelated updates per second.
const UNRELATED_RATE_PER_S: f64 = 1.0 / 60.0;

fn poisson(rng: &mut impl Rng, mean: f64) -> usize {
    let l = (-mean).exp();
    let (mut k, mut p) = (0usize, 1.0f64);
    loop {
        p *= rng.random::<f64>();
        if p <= l {
            return k;
        }
        k += 1;
    }
}

/// Count liquidations whose rule-picked candidate path differs from the true path.
pub fn misattributed(seed: u64, n: usize, delay_s: f64) -> usize {
    let mut rng = rand_pcg::Pcg64Mcg::seed_from_u64(seed);
    let mut count = 0;
    for _ in 0..n {
        let fallback = rng.random_bool(0.5);
        if !fallback {
            continue; // private path: no delay window, rule picks the true update
        }
        let k = poisson(&mut rng, delay_s * UNRELATED_RATE_PER_S);
        let pick = rng.random_range(0..=k); // 0 is the true update
        if pick > 0 && !rng.random_bool(0.5) {
            // unrelated update on the private path while the truth is fallback
            count += 1;
        } else if pick > 0 {
            // draw consumed identically either way
        }
    }
    count
}

pub fn fallbackcheck(seed: u64, n: usize, out: PathBuf) -> anyhow::Result<()> {
    let started = std::time::Instant::now();
    let started_at = chrono_now();
    let c10 = misattributed(seed, n, 10.0);
    let c60 = misattributed(seed, n, 60.0);
    let v = serde_json::json!({
        "summary": {
            "n_liquidations": n,
            "fallback_path_misattributed_count": c60,
            "fallback_path_misattributed_count_10s": c10,
        },
        "provenance": {
            "started_at": started_at,
            "seeds": [seed],
            "command": format!("cargo run --locked --release -- fallbackcheck --seed {seed} --n {n} --out results/56-fallbackcheck.json"),
            "hardware": format!("{} on {} logical cpus", std::env::consts::ARCH, std::thread::available_parallelism().map(|x| x.get()).unwrap_or(1)),
            "wall_s": started.elapsed().as_secs_f64(),
            "data_sources": [{"url": "https://governance.aave.com/t/arfc-aave-chainlink-svr-multi-network-expansion-base-arbitrum/24241", "license": "public governance forum, quoted for the problem statement only; no data used"}],
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
        assert_eq!(misattributed(7, 300, 60.0), misattributed(7, 300, 60.0));
    }

    #[test]
    fn zero_delay_has_no_misattribution() {
        assert_eq!(misattributed(7, 500, 0.0), 0);
    }

    #[test]
    fn pointer_resolves_in_written_json() {
        let p = std::env::temp_dir().join(format!("fallbackcheck-test-{}.json", std::process::id()));
        fallbackcheck(7, 50, p.clone()).unwrap();
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
        assert!(v.pointer("/summary/fallback_path_misattributed_count").unwrap().is_u64());
        assert!(v.pointer("/provenance/started_at").unwrap().is_string());
    }
}
