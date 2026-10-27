//! `attribcheck`: how many synthetic blocks a public-only same-block adjacency rule misclassifies when
//! half of the adjacent oracle-update and liquidation pairs are coincidental. The mix is an assumption
//! fixed in preregistration_additions.md; no chain data is used.

use crate::defcheck::chrono_now;
use rand::{Rng, SeedableRng};
use std::path::PathBuf;

pub struct Block {
    pub adjacent: bool,
    pub shared_origin: bool,
}

pub fn blocks(seed: u64, n: usize) -> Vec<Block> {
    let mut rng = rand_pcg::Pcg64Mcg::seed_from_u64(seed);
    (0..n).map(|_| Block { adjacent: true, shared_origin: rng.random_bool(0.5) }).collect()
}

/// Public-only rule: adjacency is the only observable, so it labels every adjacent pair as shared.
pub fn rule_says_shared(b: &Block) -> bool {
    b.adjacent
}

pub fn misclassified(b: &Block) -> bool {
    rule_says_shared(b) != b.shared_origin
}

pub fn attribcheck(seed: u64, n: usize, out: PathBuf) -> anyhow::Result<()> {
    let started = std::time::Instant::now();
    let started_at = chrono_now();
    let bl = blocks(seed, n);
    let count = bl.iter().filter(|b| misclassified(b)).count();
    let shared = bl.iter().filter(|b| b.shared_origin).count();
    let v = serde_json::json!({
        "summary": {
            "n_blocks": n,
            "adjacency_misclassified_block_count": count,
            "shared_origin_block_count": shared,
            "coincidental_block_count": n - shared,
        },
        "provenance": {
            "started_at": started_at,
            "seeds": [seed],
            "command": format!("cargo run --locked --release -- attribcheck --seed {seed} --n {n} --out results/55-attribcheck.json"),
            "hardware": format!("{} on {} logical cpus", std::env::consts::ARCH, std::thread::available_parallelism().map(|x| x.get()).unwrap_or(1)),
            "wall_s": started.elapsed().as_secs_f64(),
            "data_sources": [{"url": "https://chaoslabs.xyz/posts/svr-monitoring-platform", "license": "vendor blog, quoted for the problem statement only; no data used"}],
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
        let a = blocks(7, 200).iter().filter(|b| misclassified(b)).count();
        let b = blocks(7, 200).iter().filter(|b| misclassified(b)).count();
        assert_eq!(a, b);
    }

    #[test]
    fn misclassified_equals_coincidental() {
        for b in blocks(3, 500).iter().filter(|b| misclassified(b)) {
            assert!(!b.shared_origin);
        }
    }

    #[test]
    fn pointer_resolves_in_written_json() {
        let p = std::env::temp_dir().join(format!("attribcheck-test-{}.json", std::process::id()));
        attribcheck(7, 50, p.clone()).unwrap();
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
        assert!(v.pointer("/summary/adjacency_misclassified_block_count").unwrap().is_u64());
        assert!(v.pointer("/provenance/started_at").unwrap().is_string());
    }
}
