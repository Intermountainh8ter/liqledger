//! liqledger: an open liquidation and MEV observatory.
//!
//! Reconstructs every Aave V3, Spark and Morpho Blue liquidation on Ethereum
//! mainnet from the logs, receipts and block headers an ordinary full node
//! serves. No traces, no archive state, no third-party API. For each
//! liquidation it records who won, where in the block the transaction landed,
//! what priority fee was paid to the builder, and whether a Chainlink oracle
//! update landed in the same block or in the blocks just before.
//!
//! What a full node cannot show is written down rather than estimated: direct
//! coinbase transfers (the other half of a builder payment) need traces, so the
//! `priority_fee_wei` column is a lower bound on what the winner paid.
//! See docs/PREREGISTRATION.md.

mod follow;
mod price;

use alloy::eips::BlockId;
use alloy::primitives::{Address, B256};
use alloy::providers::{Provider, ProviderBuilder};
use alloy::rpc::types::{BlockNumberOrTag, Filter, Log, TransactionReceipt};
use alloy::sol;
use alloy::sol_types::SolEvent;
use clap::{Parser, Subcommand};
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::io::Write as _;
use std::path::PathBuf;

sol! {
    /// Aave V3 Pool (also used verbatim by Spark).
    event LiquidationCall(
        address indexed collateralAsset,
        address indexed debtAsset,
        address indexed user,
        uint256 debtToCover,
        uint256 liquidatedCollateralAmount,
        address liquidator,
        bool receiveAToken
    );

    /// Morpho Blue.
    event Liquidate(
        bytes32 indexed id,
        address indexed caller,
        address indexed borrower,
        uint256 repaidAssets,
        uint256 repaidShares,
        uint256 seizedAssets,
        uint256 badDebtAssets,
        uint256 badDebtShares
    );

    /// Chainlink aggregator (every OCR aggregator emits this on a new answer).
    event AnswerUpdated(int256 indexed current, uint256 indexed roundId, uint256 updatedAt);
}

pub(crate) const AAVE_V3_POOL: &str = "0x87870Bca3F3fD6335C3F4ce8392D69350B4fA4E2";
pub(crate) const SPARK_POOL: &str = "0xC13e21B648A5Ee794902342038FF3aDAB66BE987";
pub(crate) const MORPHO_BLUE: &str = "0xBBBBBbbBBb9cC5e90e3b3Af64bdAF62C37EEFFCb";

/// How many blocks before a liquidation to look for the most recent oracle update.
const ORACLE_LOOKBACK: u64 = 50;
/// Bootstrap resamples and seed, fixed in the pre-registration.
pub(crate) const BOOT_ITERS: usize = 2000;
pub(crate) const BOOT_SEED: u64 = 42;

#[derive(Parser)]
#[command(name = "liqledger", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Scan a block range and append one JSON record per liquidation to `--out`.
    /// Resumable: blocks already present in the output file are skipped.
    Scan {
        /// Node RPC URL. Read from LIQLEDGER_RPC_URL by default; never logged.
        #[arg(long, env = "LIQLEDGER_RPC_URL", hide_env_values = true, default_value = "http://127.0.0.1:8545")]
        rpc: String,
        #[arg(long)]
        from: u64,
        /// Inclusive. Defaults to the node's head.
        #[arg(long)]
        to: Option<u64>,
        /// Blocks per eth_getLogs request.
        #[arg(long, default_value_t = 2000)]
        chunk: u64,
        #[arg(long)]
        out: PathBuf,
    },
    /// Sample blocks uniformly in a range and record whether each contains a Chainlink update.
    /// This is the comparison population for the oracle-adjacency hypothesis (pre-reg H2).
    Baseline {
        #[arg(long, env = "LIQLEDGER_RPC_URL", hide_env_values = true, default_value = "http://127.0.0.1:8545")]
        rpc: String,
        #[arg(long)]
        from: u64,
        #[arg(long)]
        to: u64,
        #[arg(long, default_value_t = 2000)]
        n: usize,
        #[arg(long, default_value_t = BOOT_SEED)]
        seed: u64,
        #[arg(long)]
        out: PathBuf,
    },
    /// Read a records file (and optionally a baseline) and write the summary JSON the README renders from.
    Summarize {
        #[arg(long)]
        records: PathBuf,
        #[arg(long)]
        baseline: Option<PathBuf>,
        #[arg(long)]
        out: PathBuf,
    },
    /// v2 (addendum A): price Aave V3 and Spark records in USD from Chainlink AnswerUpdated logs
    /// and attribute each to its own collateral and debt feeds. Writes priced.jsonl and feeds.json.
    Price {
        #[arg(long, env = "LIQLEDGER_RPC_URL", hide_env_values = true, default_value = "http://127.0.0.1:8545")]
        rpc: String,
        #[arg(long)]
        records: PathBuf,
        #[arg(long)]
        out: PathBuf,
        #[arg(long)]
        feeds_out: PathBuf,
    },
    /// v2 summary over priced.jsonl: H4-H6, gross profit, the visible clearing-price curve.
    SummarizeV2 {
        #[arg(long)]
        priced: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    /// v2 (addendum A.3): live follower writing per-block builder-payment bounds (coinbase balance delta
    /// minus priority fees minus withdrawals) and the liquidation transactions in each block.
    Follow {
        #[arg(long, env = "LIQLEDGER_RPC_URL", hide_env_values = true, default_value = "http://127.0.0.1:8545")]
        rpc: String,
        #[arg(long)]
        out: PathBuf,
        /// Stop after this many blocks (default: run until interrupted).
        #[arg(long)]
        blocks: Option<u64>,
    },
    /// v2 (addendum A.3 / A.4 descriptive): summarise a follower file into the distribution of per-block
    /// direct transfers in liquidation blocks against all other blocks. Rows written before the
    /// proposer-payment amendment are counted and excluded.
    FollowSummarize {
        #[arg(long)]
        records: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    /// Print the event signatures this tool filters on (for the pre-registration).
    Topics,
    /// Render `summary.json` (and, if given, `summary-v2.json`) into the README between
    /// `<!-- RESULTS:START -->` and `<!-- RESULTS:END -->`.
    Report {
        #[arg(long)]
        summary: PathBuf,
        #[arg(long)]
        summary_v2: Option<PathBuf>,
        /// Addendum B summary (adapter-kind pricing), rendered as a second v2 section.
        #[arg(long)]
        summary_v2b: Option<PathBuf>,
        /// Addendum C summary (all aggregator phases), rendered as a third v2 section.
        #[arg(long)]
        summary_v2c: Option<PathBuf>,
        /// Follower summary (addendum A.3), rendered as a fourth v2 section.
        #[arg(long)]
        summary_follow: Option<PathBuf>,
        #[arg(long)]
        readme: PathBuf,
    },
}

fn report_v2(summary_v2: &PathBuf, label: &str) -> anyhow::Result<String> {
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(summary_v2)?)?;
    let ci = |c: &serde_json::Value| -> String {
        if c.is_null() { return "not evaluated".into(); }
        format!("{:.3} [{:.3}, {:.3}]", c["point"].as_f64().unwrap_or(f64::NAN), c["lo"].as_f64().unwrap_or(f64::NAN), c["hi"].as_f64().unwrap_or(f64::NAN))
    };
    let mut t = String::new();
    t.push_str(&format!(
        "\n### v2: USD valuation and per-feed attribution ({label})\n\nSource: `{}`. {} of {} records priced from Chainlink answers at the record's block (Aave V3 and Spark only; Morpho Blue out of scope).\n\n",
        summary_v2.display(), v["n_priced"], v["n_records"]
    ));
    t.push_str("| Pre-registered hypothesis | Measurement | Verdict |\n|---|---|---|\n");
    t.push_str(&format!("| H4 visible bids are small: median priority-fee share of gross profit < 0.50 | {} (n with visible fee {}) | {} |\n", ci(&v["visible_bid_share_median"]), v["n_with_visible_fee"], v["h4_visible_bids_small"].as_str().unwrap_or("?")));
    t.push_str(&format!("| H5 own-feed adjacency > 0.608 | {} | {} |\n", ci(&v["own_feed_same_block_share"]), v["h5_own_feed_adjacency"].as_str().unwrap_or("?")));
    t.push_str(&format!("| H6 top-10% of liquidations carry > 80% of gross profit | {} | {} |\n\n", ci(&v["top10pct_profit_share"]), v["h6_bonus_concentration"].as_str().unwrap_or("?")));
    t.push_str("| Descriptive | Value |\n|---|---|\n");
    t.push_str(&format!("| Gross profit, total (USD, before gas and builder payment) | {:.0} |\n", v["gross_profit_usd_total"].as_f64().unwrap_or(0.0)));
    if let Some(m) = v["gross_profit_usd_by_protocol"].as_object() {
        let parts: Vec<String> = m.iter().map(|(k, x)| format!("{k} {:.0}", x.as_f64().unwrap_or(0.0))).collect();
        t.push_str(&format!("| Gross profit by protocol (USD) | {} |\n", parts.join("; ")));
    }
    t.push_str(&format!("| Gross profit per liquidation, median / p90 (USD) | {:.0} / {:.0} |\n", v["gross_profit_usd_median"].as_f64().unwrap_or(0.0), v["gross_profit_usd_p90"].as_f64().unwrap_or(0.0)));
    t.push_str(&format!("| Median blocks since the position's own feed last updated | {} |\n", v["median_blocks_since_own_feed"].as_f64().map(|x| format!("{x:.0}")).unwrap_or("none".into())));
    if let Some(d) = v["visible_bid_share_by_size_decile"].as_array() {
        let parts: Vec<String> = d.iter().map(|r| format!("D{} (n={}) {:.3}", r[0], r[1], r[2].as_f64().unwrap_or(f64::NAN))).collect();
        t.push_str(&format!("| Median visible bid share by gross-profit decile (D1 smallest) | {} |\n", parts.join("; ")));
    }
    if let Some(b) = v["visible_bid_share_by_builder_top5"].as_array() {
        let parts: Vec<String> = b.iter().map(|r| format!("{} (n={}) {:.3}", safe_tag(r[0].as_str().unwrap_or("?")), r[1], r[2].as_f64().unwrap_or(f64::NAN))).collect();
        t.push_str(&format!("| Median visible bid share by builder, top 5 | {} |\n", parts.join("; ")));
    }
    if let Some(m) = v["unpriced_reasons"].as_object() {
        let parts: Vec<String> = m.iter().map(|(k, x)| format!("{k}: {x}")).collect();
        t.push_str(&format!("| Unpriced records, by reason | {} |\n", parts.join("; ")));
    }
    if v.get("n_priced_exact").and_then(|x| x.as_u64()).unwrap_or(0) > 0 {
        t.push_str(&format!("| Priced exactly / approximately (ratio-cap or aggregate on either side) | {} / {} |\n", v["n_priced_exact"], v["n_priced_approx"]));
        if let Some(m) = v["priced_by_kind_pair"].as_object() {
            let mut parts: Vec<(String, u64)> = m.iter().map(|(k, x)| (k.clone(), x.as_u64().unwrap_or(0))).collect();
            parts.sort_by(|a, b| b.1.cmp(&a.1));
            t.push_str(&format!("| Source kinds (collateral/debt), top 8 | {} |\n", parts.iter().take(8).map(|(k, n)| format!("{k} {n}")).collect::<Vec<_>>().join("; ")));
        }
        t.push_str(&format!("| Sensitivity, exact-priced records only (n={}): H4 median visible bid share | {} |\n", v["n_priced_exact"], ci(&v["exact_only_visible_bid_share_median"])));
        t.push_str(&format!("| Sensitivity, exact only: H5 own-feed same-block share | {} |\n", ci(&v["exact_only_own_feed_same_block_share"])));
        t.push_str(&format!("| Sensitivity, exact only: H6 top-10% profit share | {} |\n", ci(&v["exact_only_top10pct_profit_share"])));
    }
    t.push_str("\nVisible bid share counts only the priority fee; direct coinbase transfers are not in it, so every bid-share number here is a lower bound.\n");
    Ok(t)
}

/// Builder tags come from block extra-data, which is arbitrary bytes chosen by the builder. Before
/// they go into README markdown, keep a conservative character set (so `|`, `<`, `[`, backticks and
/// comment openers cannot break the table or inject HTML) and cap the length.
fn safe_tag(raw: &str) -> String {
    let mut out: String = raw
        .chars()
        .map(|c| if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.' | '/' | ':' | '(' | ')') { c } else { '?' })
        .collect();
    if out.chars().count() > 48 {
        out = out.chars().take(48).collect::<String>() + "...";
    }
    out.trim().to_string()
}

fn report_follow(path: &PathBuf) -> anyhow::Result<String> {
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path)?)?;
    let ci = |c: &serde_json::Value| -> String {
        format!("{:.4} [{:.4}, {:.4}]", c["point"].as_f64().unwrap_or(f64::NAN), c["lo"].as_f64().unwrap_or(f64::NAN), c["hi"].as_f64().unwrap_or(f64::NAN))
    };
    let f = |x: &serde_json::Value| x.as_f64().unwrap_or(f64::NAN);
    let liq = &v["direct_transfers_liquidation_blocks"];
    let oth = &v["direct_transfers_other_blocks"];
    let mut t = String::new();
    t.push_str(&format!(
        "\n### v2: builder-payment bounds from the live follower (pre-registration addendum A.3)\n\nSource: `{}` (the follower's per-block records are kept outside the repository), blocks {} to {} ({:.1} hours, {} blocks, {} containing a liquidation; {} earlier rows written before the proposer-payment amendment are excluded{}). `direct transfers` = coinbase balance delta + the builder's proposer payment - priority fees - withdrawals, per block; in a liquidation block it is an upper bound on what that liquidation paid the builder directly. Medians carry 95% percentile bootstrap intervals, 2,000 resamples, seed 42.\n\n",
        path.display(), v["first_block"], v["last_block"], f(&v["span_hours"]), v["n_blocks"], v["n_liquidation_blocks"], v["n_pre_amendment_excluded"],
        match v["pre_amendment_liquidation_blocks"].as_array() { Some(a) if !a.is_empty() => format!(", among them liquidation blocks {}", a.iter().map(|b| b.to_string()).collect::<Vec<_>>().join(", ")), _ => String::new() }
    ));
    t.push_str("| Descriptive | Liquidation blocks | Other blocks |\n|---|---|---|\n");
    t.push_str(&format!("| Blocks | {} | {} |\n", liq["n"], oth["n"]));
    t.push_str(&format!("| Direct transfers, median (ETH) | {} | {} |\n", ci(&liq["median_ci"]), ci(&oth["median_ci"])));
    t.push_str(&format!("| Direct transfers, p25 / p75 (ETH) | {:.4} / {:.4} | {:.4} / {:.4} |\n", f(&liq["p25_eth"]), f(&liq["p75_eth"]), f(&oth["p25_eth"]), f(&oth["p75_eth"])));
    t.push_str(&format!("| Direct transfers, p90 (ETH) | {:.4} | {:.4} |\n", f(&liq["p90_eth"]), f(&oth["p90_eth"])));
    t.push_str(&format!("| Direct transfers, mean (ETH) | {:.4} | {:.4} |\n", f(&liq["mean_eth"]), f(&oth["mean_eth"])));
    t.push_str(&format!("| Share of blocks with a negative figure (unidentified outflow) | {:.3} | {:.3} |\n", f(&liq["share_negative"]), f(&oth["share_negative"])));
    t.push_str(&format!("| Median difference, liquidation minus other (ETH) | {} | |\n\n", ci(&v["median_difference_eth"])));
    if let Some(a) = v["liquidation_blocks"].as_array() {
        t.push_str("| Liquidation block | Builder | Direct transfers (ETH) | Priority fees (ETH) | Proposer payment (ETH) | Liquidations |\n|---|---|---|---|---|---|\n");
        for b in a {
            t.push_str(&format!("| {} | {} | {:.4} | {:.4} | {:.4} | {} |\n", b["block"], safe_tag(b["builder"].as_str().unwrap_or("?")), f(&b["direct_transfers_eth"]), f(&b["priority_fees_eth"]), f(&b["proposer_payment_eth"]), b["n_liquidations"]));
        }
        t.push('\n');
    }
    if let Some(a) = v["by_builder_top5"].as_array() {
        let parts: Vec<String> = a.iter().map(|r| format!("{} ({} blocks, {} with a liquidation, median {:.4} ETH)", safe_tag(r[0].as_str().unwrap_or("?")), r[1], r[2], f(&r[3]))).collect();
        t.push_str(&format!("Builders with most blocks in the window: {}.\n\n", parts.join("; ")));
    }
    t.push_str("The proposer payment is identified heuristically as the block's last value-carrying transaction sent from the coinbase; a block where the builder pays differently, or spends from the coinbase for other reasons, shows up as a negative or understated figure. With a handful of liquidation blocks per day this table is descriptive and carries no verdict.\n");
    Ok(t)
}

fn report(summary: &PathBuf, summary_v2: Option<&PathBuf>, summary_v2b: Option<&PathBuf>, summary_v2c: Option<&PathBuf>, summary_follow: Option<&PathBuf>, readme: &PathBuf) -> anyhow::Result<()> {
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(summary)?)?;
    let get = |k: &str| -> anyhow::Result<&serde_json::Value> { v.get(k).ok_or_else(|| anyhow::anyhow!("summary missing `{k}`")) };
    let f = |k: &str| -> anyhow::Result<f64> { get(k)?.as_f64().ok_or_else(|| anyhow::anyhow!("`{k}` not a number")) };
    let ci = |k: &str| -> anyhow::Result<String> {
        let c = get(k)?;
        Ok(format!(
            "{:.3} [{:.3}, {:.3}]",
            c["point"].as_f64().unwrap_or(f64::NAN),
            c["lo"].as_f64().unwrap_or(f64::NAN),
            c["hi"].as_f64().unwrap_or(f64::NAN)
        ))
    };
    let ci_opt = |k: &str| -> String {
        match v.get(k) {
            Some(c) if !c.is_null() => format!(
                "{:.3} [{:.3}, {:.3}]",
                c["point"].as_f64().unwrap_or(f64::NAN),
                c["lo"].as_f64().unwrap_or(f64::NAN),
                c["hi"].as_f64().unwrap_or(f64::NAN)
            ),
            _ => "not computed".into(),
        }
    };
    let mut protos: Vec<String> = Vec::new();
    if let Some(m) = get("by_protocol")?.as_object() {
        for (k, n) in m {
            protos.push(format!("{k} {n}"));
        }
    }
    let mut builders: Vec<String> = Vec::new();
    if let Some(a) = get("builders_top5")?.as_array() {
        for b in a {
            builders.push(format!("{} ({})", safe_tag(b[0].as_str().unwrap_or("?")), b[1]));
        }
    }
    let mut t = String::new();
    t.push_str(&format!(
        "Source: `{}`, blocks {} to {} ({} liquidation events in {} transactions; {}). Shares carry 95% percentile bootstrap intervals, 2,000 resamples, seed 42.\n\n",
        summary.display(),
        get("first_block")?,
        get("last_block")?,
        get("n_liquidations")?,
        get("n_transactions")?,
        protos.join(", ")
    ));
    t.push_str("| Pre-registered hypothesis | Measurement | Verdict |\n|---|---|---|\n");
    t.push_str(&format!("| H1 concentration: top-3 liquidators win > 50% | top-3 share {} | {} |\n", ci("top3_share")?, get("h1_concentration")?.as_str().unwrap_or("?")));
    t.push_str(&format!(
        "| H2 oracle adjacency: enrichment > 1.5x | same-block oracle update {} vs baseline blocks {}; enrichment {} | {} |\n",
        ci("share_same_block_oracle_update")?,
        ci_opt("baseline_share_blocks_with_oracle_update"),
        ci_opt("oracle_enrichment_ratio"),
        get("h2_oracle_adjacency")?.as_str().unwrap_or("?")
    ));
    t.push_str(&format!("| H3 block position: tx index < 5 in > 50% | share {} | {} |\n\n", ci("share_tx_index_lt_5")?, get("h3_block_position")?.as_str().unwrap_or("?")));
    t.push_str("| Descriptive | Value |\n|---|---|\n");
    t.push_str(&format!("| Unique liquidator addresses / unique transaction senders | {} / {} |\n", get("unique_liquidators")?, get("unique_tx_senders")?));
    t.push_str(&format!("| Top-1 / top-10 liquidator share | {:.3} / {:.3} |\n", f("top1_share")?, f("top10_share")?));
    t.push_str(&format!("| Liquidator is a contract (event liquidator != tx sender) | {:.3} |\n", f("contract_liquidator_share")?));
    t.push_str(&format!(
        "| Priority fee paid per liquidation, median / p90 (ETH; lower bound, see below) | {:.5} / {:.5} |\n",
        f("median_priority_fee_gwei_total")? / 1e9,
        f("p90_priority_fee_gwei_total")? / 1e9
    ));
    t.push_str(&format!("| Priority fee exactly zero (builder paid by coinbase transfer instead) | {} |\n", ci("share_zero_priority_fee")?));
    t.push_str(&format!("| Transaction index 0 | {:.3} |\n", f("share_tx_index_0")?));
    t.push_str(&format!("| Oracle update within 2 blocks before (any feed) | {:.3} |\n", f("share_oracle_update_within_2_blocks")?));
    t.push_str(&format!(
        "| Median blocks since last oracle update (any feed, 50-block lookback) | {} |\n",
        get("median_blocks_since_oracle_update")?.as_f64().map(|x| format!("{x:.0}")).unwrap_or_else(|| "none within lookback".into())
    ));
    t.push_str(&format!("| Builders, top 5 by liquidations included | {} |\n", builders.join("; ")));
    if let Some(v2) = summary_v2 {
        t.push_str(&report_v2(v2, "pre-registration addendum A: plain Chainlink proxies only")?);
    }
    if let Some(v2b) = summary_v2b {
        t.push_str(&report_v2(v2b, "pre-registration addendum B: pricing through the protocols' adapter contracts")?);
    }
    if let Some(v2c) = summary_v2c {
        t.push_str(&report_v2(v2c, "pre-registration addendum C: all aggregator phases")?);
    }
    if let Some(fs) = summary_follow {
        t.push_str(&report_follow(fs)?);
    }
    const START: &str = "<!-- RESULTS:START -->";
    const END: &str = "<!-- RESULTS:END -->";
    let text = std::fs::read_to_string(readme)?;
    let s = text.find(START).ok_or_else(|| anyhow::anyhow!("README missing {START}"))?;
    let e = text.find(END).ok_or_else(|| anyhow::anyhow!("README missing {END}"))?;
    anyhow::ensure!(s < e, "END before START");
    let mut out = String::new();
    out.push_str(&text[..s + START.len()]);
    out.push('\n');
    out.push_str(&t);
    out.push_str(&text[e..]);
    std::fs::write(readme, out)?;
    print!("{t}");
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiquidationRecord {
    pub protocol: String,
    pub block: u64,
    pub timestamp: u64,
    pub tx_hash: String,
    pub tx_index: u64,
    pub block_tx_count: u64,
    /// The address the protocol credits as liquidator (event field).
    pub liquidator: String,
    /// The EOA that signed the transaction. Differs from `liquidator` when a contract liquidates.
    pub tx_from: String,
    pub borrower: String,
    pub collateral_asset: String,
    pub debt_asset: String,
    pub debt_repaid: String,
    pub collateral_seized: String,
    pub gas_used: u64,
    pub effective_gas_price_wei: String,
    pub base_fee_wei: String,
    /// (effective_gas_price - base_fee) * gas_used. Lower bound on the builder payment:
    /// direct coinbase transfers are invisible without traces.
    pub priority_fee_wei: String,
    pub builder: String,
    pub builder_extra_data: String,
    /// Chainlink AnswerUpdated events in the same block (any feed).
    pub oracle_updates_same_block: u64,
    /// Blocks since the most recent AnswerUpdated (any feed) within ORACLE_LOOKBACK, if any.
    pub blocks_since_oracle_update: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Baseline {
    pub from: u64,
    pub to: u64,
    pub seed: u64,
    pub n_sampled: usize,
    pub n_with_oracle_update: usize,
    pub share: f64,
    /// One flag per sampled block, in sample order, so the summary can bootstrap it.
    pub flags: Vec<bool>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Ci {
    pub point: f64,
    pub lo: f64,
    pub hi: f64,
}

#[derive(Debug, Serialize)]
pub struct Summary {
    pub tool: String,
    pub first_block: u64,
    pub last_block: u64,
    pub n_liquidations: u64,
    pub n_transactions: u64,
    pub by_protocol: BTreeMap<String, u64>,
    pub unique_liquidators: u64,
    pub unique_tx_senders: u64,
    pub top1_share: f64,
    /// Pre-reg H1: supported if lo > 0.50.
    pub top3_share: Ci,
    pub top10_share: f64,
    pub median_priority_fee_gwei_total: f64,
    pub p90_priority_fee_gwei_total: f64,
    pub share_zero_priority_fee: Ci,
    pub share_tx_index_0: f64,
    /// Pre-reg H3: supported if lo > 0.50.
    pub share_tx_index_lt_5: Ci,
    pub share_same_block_oracle_update: Ci,
    pub share_oracle_update_within_2_blocks: f64,
    pub median_blocks_since_oracle_update: Option<f64>,
    pub contract_liquidator_share: f64,
    pub builders_top5: Vec<(String, u64)>,
    pub baseline_share_blocks_with_oracle_update: Option<Ci>,
    /// Pre-reg H2: liquidation same-block share / baseline share; supported if lo > 1.5.
    pub oracle_enrichment_ratio: Option<Ci>,
    pub h1_concentration: String,
    pub h2_oracle_adjacency: String,
    pub h3_block_position: String,
}

fn addr(s: &str) -> Address {
    s.parse().expect("hard-coded address")
}

fn topics() -> Vec<(&'static str, B256)> {
    vec![
        ("LiquidationCall (Aave V3 / Spark)", LiquidationCall::SIGNATURE_HASH),
        ("Liquidate (Morpho Blue)", Liquidate::SIGNATURE_HASH),
        ("AnswerUpdated (Chainlink)", AnswerUpdated::SIGNATURE_HASH),
    ]
}

fn existing_blocks(out: &PathBuf) -> anyhow::Result<std::collections::HashSet<u64>> {
    let mut set = std::collections::HashSet::new();
    if let Ok(text) = std::fs::read_to_string(out) {
        for line in text.lines() {
            if let Ok(r) = serde_json::from_str::<LiquidationRecord>(line) {
                set.insert(r.block);
            }
        }
    }
    Ok(set)
}

struct BlockInfo {
    timestamp: u64,
    base_fee: u128,
    builder: Address,
    extra_data: String,
    tx_count: u64,
    receipts: Vec<TransactionReceipt>,
}

/// Block header plus all its receipts in one fetch. `eth_getBlockReceipts` works for every
/// block the node still has bodies for, unlike receipt-by-hash, which depends on the
/// transaction index that Geth keeps only for the most recent ~2.35M blocks.
async fn block_info<P: Provider>(p: &P, cache: &mut HashMap<u64, BlockInfo>, n: u64) -> anyhow::Result<()> {
    if cache.contains_key(&n) {
        return Ok(());
    }
    let b = p
        .get_block_by_number(BlockNumberOrTag::Number(n))
        .await?
        .ok_or_else(|| anyhow::anyhow!("block {n} missing"))?;
    let receipts = p
        .get_block_receipts(BlockId::number(n))
        .await?
        .ok_or_else(|| anyhow::anyhow!("receipts for block {n} missing"))?;
    cache.insert(
        n,
        BlockInfo {
            timestamp: b.header.timestamp,
            base_fee: b.header.base_fee_per_gas.unwrap_or(0) as u128,
            builder: b.header.beneficiary,
            extra_data: String::from_utf8_lossy(&b.header.extra_data).chars().filter(|c| !c.is_control()).collect(),
            tx_count: b.transactions.len() as u64,
            receipts,
        },
    );
    Ok(())
}

async fn oracle_update_blocks<P: Provider>(p: &P, from: u64, to: u64) -> anyhow::Result<Vec<u64>> {
    let f = Filter::new()
        .event_signature(AnswerUpdated::SIGNATURE_HASH)
        .from_block(from)
        .to_block(to);
    let logs = p.get_logs(&f).await?;
    Ok(logs.iter().filter_map(|l| l.block_number).collect())
}

async fn scan(rpc: String, from: u64, to: Option<u64>, chunk: u64, out: PathBuf) -> anyhow::Result<()> {
    let provider = ProviderBuilder::new().connect_http(rpc.parse()?);
    let head = provider.get_block_number().await?;
    let to = to.unwrap_or(head).min(head);
    anyhow::ensure!(from <= to, "from > to");
    let done = existing_blocks(&out)?;
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(&out)?;
    let mut blocks: HashMap<u64, BlockInfo> = HashMap::new();
    let mut n_written = 0u64;
    let mut n_decode_failures = 0u64;

    let pools = [addr(AAVE_V3_POOL), addr(SPARK_POOL)];
    let morpho = addr(MORPHO_BLUE);

    let mut start = from;
    while start <= to {
        let end = (start + chunk - 1).min(to);
        let f_aave = Filter::new()
            .address(pools.to_vec())
            .event_signature(LiquidationCall::SIGNATURE_HASH)
            .from_block(start)
            .to_block(end);
        let f_morpho = Filter::new()
            .address(morpho)
            .event_signature(Liquidate::SIGNATURE_HASH)
            .from_block(start)
            .to_block(end);
        let mut logs: Vec<Log> = provider.get_logs(&f_aave).await?;
        logs.extend(provider.get_logs(&f_morpho).await?);
        logs.sort_by_key(|l| (l.block_number.unwrap_or(0), l.log_index.unwrap_or(0)));

        for log in logs {
            let bn = log.block_number.ok_or_else(|| anyhow::anyhow!("log without block"))?;
            if done.contains(&bn) {
                continue;
            }
            let tx_hash = log.transaction_hash.ok_or_else(|| anyhow::anyhow!("log without tx hash"))?;
            block_info(&provider, &mut blocks, bn).await?;
            let bi = &blocks[&bn];
            let receipt = bi
                .receipts
                .iter()
                .find(|r| r.transaction_hash == tx_hash)
                .ok_or_else(|| anyhow::anyhow!("receipt {tx_hash:#x} not in block {bn} receipts"))?;
            let egp = receipt.effective_gas_price;
            let gas_used = receipt.gas_used;
            let tip = egp.saturating_sub(bi.base_fee).saturating_mul(gas_used as u128);

            let lo = bn.saturating_sub(ORACLE_LOOKBACK);
            let upd = oracle_update_blocks(&provider, lo, bn).await?;
            let same = upd.iter().filter(|&&b| b == bn).count() as u64;
            let last_before = upd.iter().filter(|&&b| b <= bn).max().copied();
            let since = last_before.map(|b| bn - b);

            let decoded = if log.address() == morpho {
                Liquidate::decode_log(&log.inner).map(|ev| {
                    (
                        "morpho-blue".to_string(),
                        format!("{:#x}", ev.caller),
                        format!("{:#x}", ev.borrower),
                        format!("market:{:#x}", ev.id),
                        format!("market:{:#x}", ev.id),
                        ev.repaidAssets.to_string(),
                        ev.seizedAssets.to_string(),
                    )
                })
            } else {
                LiquidationCall::decode_log(&log.inner).map(|ev| {
                    let proto = if log.address() == addr(AAVE_V3_POOL) { "aave-v3" } else { "spark" };
                    (
                        proto.to_string(),
                        format!("{:#x}", ev.liquidator),
                        format!("{:#x}", ev.user),
                        format!("{:#x}", ev.collateralAsset),
                        format!("{:#x}", ev.debtAsset),
                        ev.debtToCover.to_string(),
                        ev.liquidatedCollateralAmount.to_string(),
                    )
                })
            };
            let (protocol, liquidator, borrower, coll, debt, repaid, seized) = match decoded {
                Ok(v) => v,
                Err(e) => {
                    n_decode_failures += 1;
                    eprintln!("decode failure at block {bn} tx {tx_hash:#x}: {e}");
                    continue;
                }
            };

            let rec = LiquidationRecord {
                protocol,
                block: bn,
                timestamp: bi.timestamp,
                tx_hash: format!("{tx_hash:#x}"),
                tx_index: receipt.transaction_index.unwrap_or(0),
                block_tx_count: bi.tx_count,
                liquidator,
                tx_from: format!("{:#x}", receipt.from),
                borrower,
                collateral_asset: coll,
                debt_asset: debt,
                debt_repaid: repaid,
                collateral_seized: seized,
                gas_used,
                effective_gas_price_wei: egp.to_string(),
                base_fee_wei: bi.base_fee.to_string(),
                priority_fee_wei: tip.to_string(),
                builder: format!("{:#x}", bi.builder),
                builder_extra_data: bi.extra_data.clone(),
                oracle_updates_same_block: same,
                blocks_since_oracle_update: since,
            };
            writeln!(file, "{}", serde_json::to_string(&rec)?)?;
            n_written += 1;
        }
        eprintln!("scanned {start}..{end}  written so far {n_written}");
        start = end + 1;
    }
    eprintln!("done: {n_written} new records in {} ({n_decode_failures} decode failures)", out.display());
    Ok(())
}

async fn baseline(rpc: String, from: u64, to: u64, n: usize, seed: u64, out: PathBuf) -> anyhow::Result<()> {
    anyhow::ensure!(from < to, "from >= to");
    let provider = ProviderBuilder::new().connect_http(rpc.parse()?);
    let mut rng = rand_pcg::Pcg64Mcg::seed_from_u64(seed);
    let mut flags = Vec::with_capacity(n);
    for i in 0..n {
        let b: u64 = rng.random_range(from..=to);
        let upd = oracle_update_blocks(&provider, b, b).await?;
        flags.push(!upd.is_empty());
        if (i + 1) % 200 == 0 {
            eprintln!("sampled {}/{n}", i + 1);
        }
    }
    let with = flags.iter().filter(|&&f| f).count();
    let b = Baseline { from, to, seed, n_sampled: n, n_with_oracle_update: with, share: with as f64 / n as f64, flags };
    std::fs::write(&out, serde_json::to_string(&b)?)?;
    println!("baseline: {with}/{n} sampled blocks ({:.3}) contain a Chainlink AnswerUpdated", b.share);
    Ok(())
}

pub(crate) fn percentile(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return f64::NAN;
    }
    let idx = ((sorted.len() - 1) as f64 * q).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

/// Percentile bootstrap of `stat` over resampled indices 0..n. Deterministic for a seed.
pub(crate) fn bootstrap(n: usize, stat: impl Fn(&[usize]) -> f64, seed: u64) -> Ci {
    let all: Vec<usize> = (0..n).collect();
    let point = stat(&all);
    let mut rng = rand_pcg::Pcg64Mcg::seed_from_u64(seed);
    let mut vals: Vec<f64> = (0..BOOT_ITERS)
        .map(|_| {
            let idx: Vec<usize> = (0..n).map(|_| rng.random_range(0..n)).collect();
            stat(&idx)
        })
        .collect();
    // A resample can leave a statistic undefined (NaN); drop those and say so rather than panic.
    let before = vals.len();
    vals.retain(|v| !v.is_nan());
    if vals.len() < before { eprintln!("bootstrap: {} of {before} resamples undefined (NaN), excluded from the interval", before - vals.len()); }
    vals.sort_by(|a, b| a.total_cmp(b));
    Ci { point, lo: percentile(&vals, 0.025), hi: percentile(&vals, 0.975) }
}

pub(crate) fn share_of(flags: &[bool]) -> impl Fn(&[usize]) -> f64 + '_ {
    move |idx| idx.iter().filter(|&&i| flags[i]).count() as f64 / idx.len().max(1) as f64
}

fn summarize(records: PathBuf, baseline: Option<PathBuf>, out: PathBuf) -> anyhow::Result<()> {
    let text = std::fs::read_to_string(&records)?;
    let mut recs: Vec<LiquidationRecord> = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        recs.push(serde_json::from_str(line)?);
    }
    anyhow::ensure!(!recs.is_empty(), "no records in {}", records.display());
    let n = recs.len();
    let nf = n as f64;

    let mut by_protocol = BTreeMap::new();
    let mut by_liq: HashMap<&str, u64> = HashMap::new();
    let mut by_builder: HashMap<String, u64> = HashMap::new();
    let mut senders = std::collections::HashSet::new();
    let mut txs = std::collections::HashSet::new();
    let mut tips = Vec::new();
    let mut since = Vec::new();
    let mut f_idx0 = Vec::with_capacity(n);
    let mut f_idx5 = Vec::with_capacity(n);
    let mut f_same = Vec::with_capacity(n);
    let mut f_zero = Vec::with_capacity(n);
    let (mut within2, mut contract) = (0u64, 0u64);
    for r in &recs {
        *by_protocol.entry(r.protocol.clone()).or_insert(0) += 1;
        *by_liq.entry(r.liquidator.as_str()).or_insert(0) += 1;
        let tag = if r.builder_extra_data.is_empty() { r.builder.clone() } else { r.builder_extra_data.clone() };
        *by_builder.entry(tag).or_insert(0) += 1;
        senders.insert(r.tx_from.as_str());
        txs.insert(r.tx_hash.as_str());
        let tip: u128 = r.priority_fee_wei.parse().unwrap_or(0);
        tips.push(tip as f64 / 1e9);
        f_zero.push(tip == 0);
        f_idx0.push(r.tx_index == 0);
        f_idx5.push(r.tx_index < 5);
        f_same.push(r.oracle_updates_same_block > 0);
        if let Some(s) = r.blocks_since_oracle_update {
            since.push(s as f64);
            if s <= 2 {
                within2 += 1;
            }
        }
        if r.liquidator != r.tx_from {
            contract += 1;
        }
    }
    let mut counts: Vec<u64> = by_liq.values().copied().collect();
    counts.sort_unstable_by(|a, b| b.cmp(a));
    let share_top = |k: usize| counts.iter().take(k).sum::<u64>() as f64 / nf;
    let liq_names: Vec<&str> = recs.iter().map(|r| r.liquidator.as_str()).collect();
    let top3 = bootstrap(
        n,
        |idx| {
            let mut c: HashMap<&str, u64> = HashMap::new();
            for &i in idx {
                *c.entry(liq_names[i]).or_insert(0) += 1;
            }
            let mut v: Vec<u64> = c.into_values().collect();
            v.sort_unstable_by(|a, b| b.cmp(a));
            v.iter().take(3).sum::<u64>() as f64 / idx.len() as f64
        },
        BOOT_SEED,
    );
    let idx5 = bootstrap(n, share_of(&f_idx5), BOOT_SEED + 1);
    let same = bootstrap(n, share_of(&f_same), BOOT_SEED + 2);
    let zero = bootstrap(n, share_of(&f_zero), BOOT_SEED + 3);
    tips.sort_by(|a, b| a.partial_cmp(b).unwrap());
    since.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mut builders: Vec<(String, u64)> = by_builder.into_iter().collect();
    builders.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    builders.truncate(5);

    let (base_ci, enrich) = match baseline {
        Some(p) => {
            let b: Baseline = serde_json::from_str(&std::fs::read_to_string(&p)?)?;
            let bci = bootstrap(b.flags.len(), share_of(&b.flags), BOOT_SEED + 4);
            // Ratio bootstrap: resample both populations independently per iteration.
            let mut rng = rand_pcg::Pcg64Mcg::seed_from_u64(BOOT_SEED + 5);
            let mut ratios: Vec<f64> = (0..BOOT_ITERS)
                .map(|_| {
                    let a = (0..n).filter(|_| f_same[rng.random_range(0..n)]).count() as f64 / nf;
                    let m = b.flags.len();
                    let c = (0..m).filter(|_| b.flags[rng.random_range(0..m)]).count() as f64 / m as f64;
                    if c > 0.0 { a / c } else { f64::INFINITY }
                })
                .collect();
            ratios.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let point = if b.share > 0.0 { same.point / b.share } else { f64::INFINITY };
            (Some(bci), Some(Ci { point, lo: percentile(&ratios, 0.025), hi: percentile(&ratios, 0.975) }))
        }
        None => (None, None),
    };

    let enough = n >= 100;
    let verdict = |ok: bool, what: &str| -> String {
        if !enough {
            format!("not evaluated: {n} records < 100")
        } else if ok {
            format!("supported: {what}")
        } else {
            format!("not supported: {what}")
        }
    };
    let s = Summary {
        tool: format!("liqledger {}", env!("CARGO_PKG_VERSION")),
        first_block: recs.iter().map(|r| r.block).min().unwrap_or(0),
        last_block: recs.iter().map(|r| r.block).max().unwrap_or(0),
        n_liquidations: n as u64,
        n_transactions: txs.len() as u64,
        by_protocol,
        unique_liquidators: by_liq.len() as u64,
        unique_tx_senders: senders.len() as u64,
        top1_share: share_top(1),
        top3_share: top3,
        top10_share: share_top(10),
        median_priority_fee_gwei_total: percentile(&tips, 0.5),
        p90_priority_fee_gwei_total: percentile(&tips, 0.9),
        share_zero_priority_fee: zero,
        share_tx_index_0: f_idx0.iter().filter(|&&f| f).count() as f64 / nf,
        share_tx_index_lt_5: idx5,
        share_same_block_oracle_update: same,
        share_oracle_update_within_2_blocks: within2 as f64 / nf,
        median_blocks_since_oracle_update: if since.is_empty() { None } else { Some(percentile(&since, 0.5)) },
        contract_liquidator_share: contract as f64 / nf,
        builders_top5: builders,
        baseline_share_blocks_with_oracle_update: base_ci,
        oracle_enrichment_ratio: enrich,
        h1_concentration: verdict(top3.lo > 0.50, &format!("top-3 share {:.3} [{:.3}, {:.3}] vs lower bound > 0.50", top3.point, top3.lo, top3.hi)),
        h2_oracle_adjacency: match enrich {
            Some(e) => verdict(e.lo > 1.5, &format!("enrichment {:.2} [{:.2}, {:.2}] vs lower bound > 1.5", e.point, e.lo, e.hi)),
            None => "not evaluated: no baseline supplied".into(),
        },
        h3_block_position: verdict(idx5.lo > 0.50, &format!("tx index < 5 share {:.3} [{:.3}, {:.3}] vs lower bound > 0.50", idx5.point, idx5.lo, idx5.hi)),
    };
    std::fs::write(&out, serde_json::to_string_pretty(&s)?)?;
    println!("{}", serde_json::to_string_pretty(&s)?);
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Cmd::Scan { rpc, from, to, chunk, out } => scan(rpc, from, to, chunk, out).await,
        Cmd::Baseline { rpc, from, to, n, seed, out } => baseline(rpc, from, to, n, seed, out).await,
        Cmd::Summarize { records, baseline, out } => summarize(records, baseline, out),
        Cmd::Price { rpc, records, out, feeds_out } => price::price(rpc, records, out, feeds_out).await,
        Cmd::SummarizeV2 { priced, out } => price::summarize_v2(priced, out),
        Cmd::Follow { rpc, out, blocks } => follow::follow(rpc, out, blocks).await,
        Cmd::Topics => {
            for (name, h) in topics() {
                println!("{h:#x}  {name}");
            }
            Ok(())
        }
        Cmd::FollowSummarize { records, out } => follow::follow_summarize(records, out),
        Cmd::Report { summary, summary_v2, summary_v2b, summary_v2c, summary_follow, readme } => report(&summary, summary_v2.as_ref(), summary_v2b.as_ref(), summary_v2c.as_ref(), summary_follow.as_ref(), &readme),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn safe_tag_neutralises_markdown_and_html() {
        assert_eq!(super::safe_tag("Titan (titanbuilder.xyz)"), "Titan (titanbuilder.xyz)");
        assert_eq!(super::safe_tag("a|b<!--c-->`d`[e](f)"), "a?b??--c--??d??e?(f)");
        assert!(super::safe_tag(&"x".repeat(100)).ends_with("..."));
    }

    use super::*;

    #[test]
    fn aave_liquidation_topic_matches_known_hash() {
        assert_eq!(
            format!("{:#x}", LiquidationCall::SIGNATURE_HASH),
            "0xe413a321e8681d831f4dbccbca790d2952b56f977908e45be37335533e005286"
        );
    }

    #[test]
    fn chainlink_answer_updated_topic_matches_known_hash() {
        assert_eq!(
            format!("{:#x}", AnswerUpdated::SIGNATURE_HASH),
            "0x0559884fd3a460db3073b7fc896cc77986f16e378210ded43186175bf646fc5f"
        );
    }

    #[test]
    fn percentile_is_inclusive_and_monotone() {
        let v = [1.0, 2.0, 3.0, 4.0, 5.0];
        assert_eq!(percentile(&v, 0.0), 1.0);
        assert_eq!(percentile(&v, 0.5), 3.0);
        assert_eq!(percentile(&v, 1.0), 5.0);
        assert!(percentile(&[], 0.5).is_nan());
    }

    #[test]
    fn bootstrap_is_deterministic_and_brackets_the_point() {
        let flags: Vec<bool> = (0..200).map(|i| i % 4 == 0).collect();
        let a = bootstrap(flags.len(), share_of(&flags), 7);
        let b = bootstrap(flags.len(), share_of(&flags), 7);
        assert_eq!((a.lo, a.hi), (b.lo, b.hi));
        assert!((a.point - 0.25).abs() < 1e-12);
        assert!(a.lo <= a.point && a.point <= a.hi);
        assert!(a.lo > 0.15 && a.hi < 0.35);
    }

    fn rec(liq: &str, idx: u64, same: u64, since: Option<u64>, tip: &str) -> LiquidationRecord {
        LiquidationRecord {
            protocol: "aave-v3".into(),
            block: 1,
            timestamp: 0,
            tx_hash: format!("0x{liq}{idx}"),
            tx_index: idx,
            block_tx_count: 100,
            liquidator: liq.into(),
            tx_from: "0xeoa".into(),
            borrower: "0xb".into(),
            collateral_asset: "0xc".into(),
            debt_asset: "0xd".into(),
            debt_repaid: "1".into(),
            collateral_seized: "1".into(),
            gas_used: 1,
            effective_gas_price_wei: "2".into(),
            base_fee_wei: "1".into(),
            priority_fee_wei: tip.into(),
            builder: "0x0".into(),
            builder_extra_data: "Titan (titanbuilder.xyz)".into(),
            oracle_updates_same_block: same,
            blocks_since_oracle_update: since,
        }
    }

    #[test]
    fn summary_shares_and_small_n_verdicts() {
        let dir = std::env::temp_dir().join(format!("liqledger-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let recs = dir.join("r.jsonl");
        let lines: Vec<String> = [rec("0xa", 0, 1, Some(0), "1000000000"), rec("0xa", 3, 0, Some(2), "0"), rec("0xb", 40, 0, None, "0")]
            .iter()
            .map(|r| serde_json::to_string(r).unwrap())
            .collect();
        std::fs::write(&recs, lines.join("\n")).unwrap();
        let out = dir.join("s.json");
        summarize(recs, None, out.clone()).unwrap();
        let s: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(out).unwrap()).unwrap();
        assert_eq!(s["n_liquidations"], 3);
        assert!((s["top3_share"]["point"].as_f64().unwrap() - 1.0).abs() < 1e-9);
        assert!((s["share_tx_index_0"].as_f64().unwrap() - 1.0 / 3.0).abs() < 1e-9);
        assert!((s["share_same_block_oracle_update"]["point"].as_f64().unwrap() - 1.0 / 3.0).abs() < 1e-9);
        assert!((s["share_zero_priority_fee"]["point"].as_f64().unwrap() - 2.0 / 3.0).abs() < 1e-9);
        assert!((s["contract_liquidator_share"].as_f64().unwrap() - 1.0).abs() < 1e-9);
        assert!(s["h1_concentration"].as_str().unwrap().starts_with("not evaluated"));
        assert_eq!(s["h2_oracle_adjacency"], "not evaluated: no baseline supplied");
        assert_eq!(s["builders_top5"][0][0], "Titan (titanbuilder.xyz)");
    }
}
