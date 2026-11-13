//! `reviewerdet`: determinism and faithfulness of a scripted reviewer, counted separately. Parameters are fixed in
//! preregistration_additions.md; all inputs are synthetic.

use crate::defcheck::chrono_now;
use rand::{Rng, SeedableRng};
use std::path::PathBuf;

const RUNS: usize = 5;
const P_SYSTEMATIC: f64 = 0.10;
const P_NOISE: f64 = 0.05;

#[derive(Debug, PartialEq, Eq)]
pub struct Counts {
    pub run_disagreement: usize,
    pub wrong_vs_truth: usize,
}

pub fn counts(seed: u64, n: usize, p_noise: f64) -> Counts {
    let mut rng = rand_pcg::Pcg64Mcg::seed_from_u64(seed);
    let mut c = Counts { run_disagreement: 0, wrong_vs_truth: 0 };
    for _ in 0..n {
        let truth: bool = rng.random();
        let systematic = rng.random::<f64>() < P_SYSTEMATIC;
        let base = if systematic { !truth } else { truth };
        let labels: Vec<bool> = (0..RUNS)
            .map(|_| if rng.random::<f64>() < p_noise { !base } else { base })
            .collect();
        if labels.iter().any(|l| *l != labels[0]) {
            c.run_disagreement += 1;
        }
        let majority = labels.iter().filter(|l| **l).count() * 2 > RUNS;
        if majority != truth {
            c.wrong_vs_truth += 1;
        }
    }
    c
}

pub fn reviewerdet(seed: u64, n: usize, out: PathBuf) -> anyhow::Result<()> {
    let started = std::time::Instant::now();
    let started_at = chrono_now();
    let c = counts(seed, n, 0.0);
    let noisy = counts(seed, n, P_NOISE);
    let v = serde_json::json!({
        "n_fixtures": n,
        "reviewer": {
            "run_disagreement_count": c.run_disagreement,
            "wrong_vs_truth_count": c.wrong_vs_truth,
            "noisy_run_disagreement_count": noisy.run_disagreement,
            "noisy_wrong_vs_truth_count": noisy.wrong_vs_truth,
        },
        "provenance": {
            "started_at": started_at,
            "seeds": [seed],
            "command": format!("cargo run --locked -q --release -- reviewerdet --seed {seed} --n {n} --out results/131-reviewerdet.json"),
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
        assert_eq!(counts(7, 500, 0.0), counts(7, 500, 0.0));
    }

    #[test]
    fn repeatable_but_unfaithful() {
        let c = counts(7, 1000, 0.0);
        assert_eq!(c.run_disagreement, 0);
        assert!(c.wrong_vs_truth >= 80);
        assert!(counts(7, 1000, P_NOISE).run_disagreement > 0);
    }

    #[test]
    fn pointer_resolves_in_written_json() {
        let p = std::env::temp_dir().join(format!("reviewerdet-test-{}.json", std::process::id()));
        reviewerdet(7, 50, p.clone()).unwrap();
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
        assert!(v.pointer("/reviewer/run_disagreement_count").unwrap().is_u64());
        assert!(v.pointer("/provenance/started_at").unwrap().is_string());
    }
}
