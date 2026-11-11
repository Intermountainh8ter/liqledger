//! `targethf`: repay sizing to health factor 1.0 versus a per-row target on toy positions.
//! Model is fixed in preregistration_additions.md; no chain data is used.

use crate::defcheck::chrono_now;
use rand::{Rng, SeedableRng};
use std::path::PathBuf;

const LT_BPS: u128 = 8000;
const BONUS_BPS: u128 = 10500;

/// Smallest integer repay x with post health factor >= target (bps), capped at debt.
fn size(c: u128, d: u128, t_bps: u128) -> u128 {
    // HF' = (C - 1.05x) * 0.8 / (D - x) >= T  <=>  x * (T - 8400) >= T*D - 8000*C
    let need = (t_bps * d).saturating_sub(LT_BPS * c);
    let den = t_bps - LT_BPS * BONUS_BPS / 10000;
    let mut x = need.div_ceil(den).min(d);
    // the closed form ignores the floor on seized collateral; settle on the smallest integer that meets the target
    while x < d && below(c, d, x, t_bps) {
        x += 1;
    }
    while x > 0 && !below(c, d, x - 1, t_bps) {
        x -= 1;
    }
    x
}

/// True when post-liquidation health factor is strictly below the target.
fn below(c: u128, d: u128, x: u128, t_bps: u128) -> bool {
    let c2 = c - x * BONUS_BPS / 10000;
    c2 * LT_BPS < t_bps * (d - x)
}

/// (off_target mismatched, off_target matched, rows where matched repay could be one unit smaller)
pub fn counts(seed: u64, n: usize) -> [usize; 3] {
    let mut rng = rand_pcg::Pcg64Mcg::seed_from_u64(seed);
    let mut r = [0usize; 3];
    for _ in 0..n {
        let c: u128 = rng.random_range(1_000_000u128..1_000_000_000_000);
        let hf0: u128 = rng.random_range(9000u128..10000);
        let t: u128 = rng.random_range(10100u128..=10500);
        let d = c * LT_BPS / hf0;
        let xm = size(c, d, 10000);
        r[0] += below(c, d, xm, t) as usize;
        let xt = size(c, d, t);
        r[1] += below(c, d, xt, t) as usize;
        r[2] += (xt > 0 && !below(c, d, xt - 1, t)) as usize;
    }
    r
}

pub fn targethf(seed: u64, n: usize, out: PathBuf) -> anyhow::Result<()> {
    let started = std::time::Instant::now();
    let started_at = chrono_now();
    let r = counts(seed, n);
    let v = serde_json::json!({
        "n_positions": n,
        "off_target_count": r[0],
        "matched_off_target_count": r[1],
        "matched_overshoot_count": r[2],
        "provenance": {
            "started_at": started_at,
            "seeds": [seed],
            "command": format!("cargo run --locked --release -- targethf --seed {seed} --n {n} --out results/90-targethf.json"),
            "hardware": format!("{} on {} logical cpus", std::env::consts::ARCH, std::thread::available_parallelism().map(|x| x.get()).unwrap_or(1)),
            "wall_s": started.elapsed().as_secs_f64(),
            "data_sources": [{"url": "https://aave.com/docs/aave-v4/positions/liquidations", "license": "public vendor docs, quoted for rule definition only; no data used"}],
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
    fn matched_sizing_never_below_target() {
        assert_eq!(counts(3, 2000)[1], 0);
    }

    #[test]
    fn matched_sizing_is_minimal() {
        assert_eq!(counts(3, 2000)[2], 0);
    }

    #[test]
    fn pointer_resolves_in_written_json() {
        let p = std::env::temp_dir().join(format!("targethf-test-{}.json", std::process::id()));
        targethf(7, 200, p.clone()).unwrap();
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
        assert!(v.pointer("/off_target_count").unwrap().is_u64());
        assert!(v.pointer("/provenance/started_at").unwrap().is_string());
    }
}
