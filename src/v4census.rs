//! `v4census`: count Aave V4 `LiquidationCall` logs emitted by the Spokes listed in the bgd-labs address book
//! over a pinned Ethereum block range. Parameters are fixed in preregistration_additions.md. Reads only; the
//! endpoint is resolved from the Chainstack platform API at run time and is never printed. No addresses are written.

use crate::defcheck::chrono_now;
use crate::mainoracleimmediate::{chainstack_endpoint, logs_retry};
use alloy::primitives::Address;
use alloy::providers::ProviderBuilder;
use alloy::rpc::types::{Filter, Log};
use alloy::sol;
use alloy::sol_types::SolEvent;
use std::collections::BTreeSet;
use std::path::PathBuf;

sol! {
    struct PremiumDelta { int256 sharesDelta; int256 offsetRayDelta; uint256 restoredPremiumRay; }
    event V4LiquidationCall(
        uint256 indexed collateralReserveId,
        uint256 indexed debtReserveId,
        address indexed user,
        address liquidator,
        bool receiveShares,
        uint256 debtAmountRestored,
        uint256 drawnSharesLiquidated,
        PremiumDelta premiumDelta,
        uint256 collateralAmountRemoved,
        uint256 collateralSharesLiquidated,
        uint256 collateralSharesToLiquidator
    );
}

/// Spoke (name, address) pairs from the `AaveV4EthereumSpokes` library: constants typed `ISpoke`, minus any
/// name containing ORACLE, TREASURY or TOKENIZATION.
pub fn parse_spokes(src: &str) -> Vec<(String, String)> {
    let body = match src.find("library AaveV4EthereumSpokes") {
        Some(i) => &src[i..],
        None => return vec![],
    };
    let flat: String = body.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out = Vec::new();
    for part in flat.split(';') {
        let Some(i) = part.find("ISpoke internal constant ") else { continue };
        let rest = &part[i + "ISpoke internal constant ".len()..];
        let Some((name, rhs)) = rest.split_once('=') else { continue };
        let name = name.trim().to_string();
        let up = name.to_uppercase();
        if up.contains("ORACLE") || up.contains("TREASURY") || up.contains("TOKENIZATION") {
            continue;
        }
        let Some(h) = rhs.find("0x") else { continue };
        let a: String = rhs[h..].chars().take(42).collect();
        if a.len() == 42 && a[2..].chars().all(|c| c.is_ascii_hexdigit()) {
            out.push((name, a));
        }
    }
    out
}

fn fetch_source(commit: &str) -> anyhow::Result<String> {
    let url = format!("https://raw.githubusercontent.com/bgd-labs/aave-address-book/{commit}/src/AaveV4Ethereum.sol");
    let out = std::process::Command::new("curl")
        .args(["-sS", "-f", "-A", "liqledger/0.1 curl", &url])
        .output()
        .map_err(|_| anyhow::anyhow!("address book fetch could not start curl"))?;
    anyhow::ensure!(out.status.success(), "address book fetch failed");
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// (event count, distinct transactions, distinct liquidators, per-spoke-index counts).
pub fn tally(logs: &[Log], spokes: &[Address]) -> (u64, u64, u64, Vec<u64>) {
    let mut per = vec![0u64; spokes.len()];
    let mut txs = BTreeSet::new();
    let mut liqs = BTreeSet::new();
    for l in logs {
        if let Some(i) = spokes.iter().position(|a| *a == l.address()) {
            per[i] += 1;
        }
        if let Some(t) = l.transaction_hash {
            txs.insert(t);
        }
        if let Ok(ev) = V4LiquidationCall::decode_log(&l.inner) {
            liqs.insert(ev.data.liquidator);
        }
    }
    (logs.len() as u64, txs.len() as u64, liqs.len() as u64, per)
}

pub async fn v4census(network: String, start: u64, blocks: u64, commit: String, out: PathBuf) -> anyhow::Result<()> {
    let started = std::time::Instant::now();
    let started_at = chrono_now();
    anyhow::ensure!(blocks >= 1, "blocks must be at least 1");
    let last = start + blocks - 1;
    let found = parse_spokes(&fetch_source(&commit)?);
    let spokes: Vec<Address> = found.iter().filter_map(|(_, a)| a.parse().ok()).collect();
    let (count, ntx, nliq, per) = if spokes.is_empty() {
        (0, 0, 0, vec![])
    } else {
        let url = chainstack_endpoint(&network)?;
        let p = ProviderBuilder::new().connect_http(url.parse().map_err(|_| anyhow::anyhow!("bad {network} endpoint"))?);
        let mut logs = Vec::new();
        let mut a = start;
        while a <= last {
            let z = (a + 9_999).min(last);
            let f = Filter::new().address(spokes.clone()).event_signature(V4LiquidationCall::SIGNATURE_HASH).from_block(a).to_block(z);
            logs.extend(logs_retry(&p, &f).await?);
            a = z + 1;
        }
        tally(&logs, &spokes)
    };
    let per_spoke: Vec<_> = found.iter().zip(per.iter().chain(std::iter::repeat(&0))).map(|((n, _), c)| serde_json::json!({"spoke": n, "events": c})).collect();
    let v = serde_json::json!({
        "v4_liquidation_event_count": count,
        "v4_liquidation_tx_count": ntx,
        "v4_distinct_liquidator_count": nliq,
        "spoke_count": spokes.len(),
        "per_spoke": per_spoke,
        "first_block": start, "last_block": last, "blocks": blocks,
        "address_book_commit": commit,
        "provenance": {
            "started_at": started_at,
            "seeds": [7],
            "command": format!("v4census --chainstack-network {network} --start {start} --blocks {blocks} --address-book-commit {commit} --out {}", out.display()),
            "hardware": format!("{} on {} logical cpus", std::env::consts::ARCH, std::thread::available_parallelism().map(|x| x.get()).unwrap_or(1)),
            "wall_s": started.elapsed().as_secs_f64(),
            "data_sources": [
                {"url": format!("chainstack:{network} blocks {start} to {last} (logs)"), "license": "public chain data via the owner's Chainstack node"},
                {"url": format!("https://raw.githubusercontent.com/bgd-labs/aave-address-book/{commit}/src/AaveV4Ethereum.sol"), "license": "MIT (bgd-labs aave-address-book)"},
                {"url": "https://raw.githubusercontent.com/aave/aave-v4/main/src/spoke/interfaces/ISpoke.sol", "license": "public source (Aave V4 repository)"}
            ],
        }
    });
    std::fs::write(&out, serde_json::to_string_pretty(&v)? + "\n")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_spokes_and_excludes_non_spokes() {
        let a = format!("0x{}", "ab".repeat(20));
        let b = format!("0x{}", "cd".repeat(20));
        let src = format!(
            "library Other {{ ISpoke internal constant X = ISpoke({a}); }}\nlibrary AaveV4EthereumSpokes {{\n  ITreasurySpoke internal constant TREASURY_SPOKE =\n    ITreasurySpoke({a});\n  ISpoke internal constant MAIN_SPOKE =\n    ISpoke({b});\n  IAaveOracle internal constant MAIN_SPOKE_ORACLE = IAaveOracle({a});\n}}"
        );
        let r = parse_spokes(&src);
        assert_eq!(r, vec![("MAIN_SPOKE".to_string(), b)]);
        assert!(parse_spokes("nothing").is_empty());
    }

    #[test]
    fn tally_empty() {
        assert_eq!(tally(&[], &[]), (0, 0, 0, vec![]));
    }
}
