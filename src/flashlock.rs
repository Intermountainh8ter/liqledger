//! `flashlock`: how often does sequential locked-slot ordering differ from whole-block fee ordering.
//! Model is fixed in preregistration_additions.md; no chain data is used.

use crate::defcheck::chrono_now;
use rand::{Rng, SeedableRng};
use std::path::PathBuf;

pub const SLOTS: usize = 10;

/// (rank_changed_bids, blocks_with_rank_change, blocks_first_lander_changed, total_bids)
pub fn counts(seed: u64, n: usize) -> [usize; 4] {
    let mut rng = rand_pcg::Pcg64Mcg::seed_from_u64(seed);
    let mut c = [0usize; 4];
    for _ in 0..n {
        let k = rng.random_range(2..=5usize);
        let bids: Vec<(f64, f64)> = (0..k)
            .map(|_| (rng.random::<f64>(), rng.random_range(1.0..100.0)))
            .collect();
        let mut by_fee: Vec<usize> = (0..k).collect();
        by_fee.sort_by(|&a, &b| bids[b].1.partial_cmp(&bids[a].1).unwrap());
        let slot = |i: usize| ((bids[i].0 * SLOTS as f64) as usize).min(SLOTS - 1);
        let mut by_slot: Vec<usize> = (0..k).collect();
        by_slot.sort_by(|&a, &b| {
            slot(a).cmp(&slot(b)).then(bids[b].1.partial_cmp(&bids[a].1).unwrap())
        });
        let changed = (0..k).filter(|&p| by_fee[p] != by_slot[p]).count();
        c[0] += changed;
        if changed > 0 {
            c[1] += 1;
        }
        if by_fee[0] != by_slot[0] {
            c[2] += 1;
        }
        c[3] += k;
    }
    c
}

pub fn flashlock(seed: u64, n: usize, out: PathBuf) -> anyhow::Result<()> {
    let started = std::time::Instant::now();
    let started_at = chrono_now();
    let c = counts(seed, n);
    let v = serde_json::json!({
        "n_blocks": n,
        "slots_per_block": SLOTS,
        "ordering": {
            "rank_changed_bid_count": c[0],
            "rank_changed_block_count": c[1],
            "first_lander_changed_block_count": c[2],
            "total_bid_count": c[3],
        },
        "provenance": {
            "started_at": started_at,
            "seeds": [seed],
            "command": format!("cargo run --locked --release -- flashlock --seed {seed} --n {n} --out results/83-flashlock.json"),
            "hardware": format!("{} on {} logical cpus", std::env::consts::ARCH, std::thread::available_parallelism().map(|x| x.get()).unwrap_or(1)),
            "wall_s": started.elapsed().as_secs_f64(),
            "data_sources": [{"url": "https://docs.base.org/base-chain/network-information/transaction-ordering.md", "license": "official docs, cited for the ordering-lock definition only; all data synthetic and seeded"}],
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
    fn counts_are_consistent() {
        let c = counts(3, 2000);
        assert!(c[2] <= c[1] && c[1] <= c[0] && c[0] <= c[3]);
    }

    #[test]
    fn pointer_resolves_in_written_json() {
        let p = std::env::temp_dir().join(format!("flashlock-test-{}.json", std::process::id()));
        flashlock(7, 200, p.clone()).unwrap();
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
        assert!(v.pointer("/ordering/rank_changed_bid_count").unwrap().is_u64());
        assert!(v.pointer("/provenance/started_at").unwrap().is_string());
    }
}
