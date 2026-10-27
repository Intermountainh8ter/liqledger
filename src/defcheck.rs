//! `defcheck`: how often do share definitions disagree on synthetic payment fixtures.
//! The channel mix is an assumption fixed in preregistration_additions.md; no chain data is used.

use rand::{Rng, SeedableRng};
use std::path::PathBuf;

pub const THRESHOLD: f64 = 0.05;

pub struct Fixture {
    pub gas_only: f64,
    pub all_channel: f64,
    pub share_of_bonus: f64,
}

pub fn fixtures(seed: u64, n: usize) -> Vec<Fixture> {
    let mut rng = rand_pcg::Pcg64Mcg::seed_from_u64(seed);
    (0..n)
        .map(|_| {
            let prio: f64 = rng.random_range(0.0..0.5);
            let cb = if rng.random_bool(0.5) { rng.random_range(0.0..0.6) } else { 0.0 };
            let tip = if rng.random_bool(0.1) { rng.random_range(0.0..0.2) } else { 0.0 };
            let fee = if rng.random_bool(0.3) { 0.1 } else { 0.0 };
            Fixture { gas_only: prio, all_channel: prio + cb + tip, share_of_bonus: prio + cb + tip + fee }
        })
        .collect()
}

pub fn count_disagree(fx: &[Fixture], a: impl Fn(&Fixture) -> f64, b: impl Fn(&Fixture) -> f64) -> usize {
    fx.iter().filter(|f| (a(f) - b(f)).abs() > THRESHOLD).count()
}

pub fn defcheck(seed: u64, n: usize, out: PathBuf) -> anyhow::Result<()> {
    let started = std::time::Instant::now();
    let started_at = chrono_now();
    let fx = fixtures(seed, n);
    let gas_vs_all = count_disagree(&fx, |f| f.gas_only, |f| f.all_channel);
    let gas_vs_bonus = count_disagree(&fx, |f| f.gas_only, |f| f.share_of_bonus);
    let all_vs_bonus = count_disagree(&fx, |f| f.all_channel, |f| f.share_of_bonus);
    let v = serde_json::json!({
        "summary": {
            "n_fixtures": n,
            "threshold": THRESHOLD,
            "definition_disagreement_fixture_count": gas_vs_all,
            "gas_vs_bonus_disagreement_count": gas_vs_bonus,
            "all_channel_vs_bonus_disagreement_count": all_vs_bonus,
        },
        "provenance": {
            "started_at": started_at,
            "seeds": [seed],
            "command": format!("cargo run --locked --release -- defcheck --seed {seed} --n {n} --out results/50-defcheck.json"),
            "hardware": format!("{} on {} logical cpus", std::env::consts::ARCH, std::thread::available_parallelism().map(|x| x.get()).unwrap_or(1)),
            "wall_s": started.elapsed().as_secs_f64(),
            "data_sources": [{"url": "https://governance.aave.com/t/arfc-svr-expansion-next-phase-of-multi-network-expansion/25332/2", "license": "public forum post, quoted for definitions only; no data used"}],
        }
    });
    std::fs::write(out, serde_json::to_string_pretty(&v)? + "\n")?;
    Ok(())
}

fn chrono_now() -> String {
    let s = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;
    let (days, rem) = (s.div_euclid(86400), s.rem_euclid(86400));
    // civil-from-days (Howard Hinnant)
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rem / 3600, rem % 3600 / 60, rem % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_for_seed() {
        let a = count_disagree(&fixtures(7, 200), |f| f.gas_only, |f| f.all_channel);
        let b = count_disagree(&fixtures(7, 200), |f| f.gas_only, |f| f.all_channel);
        assert_eq!(a, b);
    }

    #[test]
    fn identical_definitions_never_disagree() {
        assert_eq!(count_disagree(&fixtures(1, 100), |f| f.gas_only, |f| f.gas_only), 0);
    }

    #[test]
    fn pointer_resolves_in_written_json() {
        let p = std::env::temp_dir().join(format!("defcheck-test-{}.json", std::process::id()));
        defcheck(7, 50, p.clone()).unwrap();
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
        assert!(v.pointer("/summary/definition_disagreement_fixture_count").unwrap().is_u64());
        assert!(v.pointer("/provenance/started_at").unwrap().is_string());
    }

    #[test]
    fn timestamp_format() {
        let t = chrono_now();
        assert_eq!(t.len(), 20);
        assert!(t.ends_with('Z'));
    }
}
