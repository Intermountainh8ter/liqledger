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

use alloy::primitives::{Address, B256};
use alloy::providers::{Provider, ProviderBuilder};
use alloy::rpc::types::{BlockNumberOrTag, Filter, Log};
use alloy::sol;
use alloy::sol_types::SolEvent;
use clap::{Parser, Subcommand};
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

const AAVE_V3_POOL: &str = "0x87870Bca3F3fD6335C3F4ce8392D69350B4fA4E2";
const SPARK_POOL: &str = "0xC13e21B648A5Ee794902342038FF3aDAB66BE987";
const MORPHO_BLUE: &str = "0xBBBBBbbBBb9cC5e90e3b3Af64bdAF62C37EEFFCb";

/// How many blocks before a liquidation to look for the most recent oracle update.
const ORACLE_LOOKBACK: u64 = 50;

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
        /// Node RPC URL. Read from the LIQLEDGER_RPC_URL environment variable by default; never logged.
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
    /// Read a records file and write the summary JSON the README renders from.
    Summarize {
        #[arg(long)]
        records: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    /// Print the event signatures this tool filters on (for the pre-registration).
    Topics,
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

#[derive(Debug, Serialize)]
pub struct Summary {
    pub tool: String,
    pub first_block: u64,
    pub last_block: u64,
    pub n_liquidations: u64,
    pub by_protocol: BTreeMap<String, u64>,
    pub unique_liquidators: u64,
    pub unique_tx_senders: u64,
    /// Share of liquidations won by the top 1 / top 3 / top 10 `liquidator` addresses.
    pub top1_share: f64,
    pub top3_share: f64,
    pub top10_share: f64,
    pub median_priority_fee_gwei_total: f64,
    pub p90_priority_fee_gwei_total: f64,
    pub share_tx_index_0: f64,
    pub share_tx_index_lt_5: f64,
    pub share_same_block_oracle_update: f64,
    pub share_oracle_update_within_2_blocks: f64,
    pub median_blocks_since_oracle_update: Option<f64>,
    pub contract_liquidator_share: f64,
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
}

async fn block_info<P: Provider>(p: &P, cache: &mut HashMap<u64, BlockInfo>, n: u64) -> anyhow::Result<()> {
    if cache.contains_key(&n) {
        return Ok(());
    }
    let b = p
        .get_block_by_number(BlockNumberOrTag::Number(n))
        .await?
        .ok_or_else(|| anyhow::anyhow!("block {n} missing"))?;
    cache.insert(
        n,
        BlockInfo {
            timestamp: b.header.timestamp,
            base_fee: b.header.base_fee_per_gas.unwrap_or(0) as u128,
            builder: b.header.beneficiary,
            extra_data: String::from_utf8_lossy(&b.header.extra_data).chars().filter(|c| !c.is_control()).collect(),
            tx_count: b.transactions.len() as u64,
        },
    );
    Ok(())
}

async fn oracle_updates<P: Provider>(p: &P, from: u64, to: u64) -> anyhow::Result<Vec<u64>> {
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
            let receipt = provider
                .get_transaction_receipt(tx_hash)
                .await?
                .ok_or_else(|| anyhow::anyhow!("receipt {tx_hash} missing"))?;
            let egp = receipt.effective_gas_price;
            let gas_used = receipt.gas_used;
            let tip = egp.saturating_sub(bi.base_fee).saturating_mul(gas_used as u128);

            // Oracle context: same block and the ORACLE_LOOKBACK blocks before it.
            let lo = bn.saturating_sub(ORACLE_LOOKBACK);
            let upd = oracle_updates(&provider, lo, bn).await?;
            let same = upd.iter().filter(|&&b| b == bn).count() as u64;
            let last_before = upd.iter().filter(|&&b| b <= bn).max().copied();
            let since = last_before.map(|b| bn - b);

            let (protocol, liquidator, borrower, coll, debt, repaid, seized) = if log.address() == morpho {
                let ev = Liquidate::decode_log(&log.inner)?;
                (
                    "morpho-blue".to_string(),
                    format!("{:#x}", ev.caller),
                    format!("{:#x}", ev.borrower),
                    format!("market:{:#x}", ev.id),
                    format!("market:{:#x}", ev.id),
                    ev.repaidAssets.to_string(),
                    ev.seizedAssets.to_string(),
                )
            } else {
                let ev = LiquidationCall::decode_log(&log.inner)?;
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
    eprintln!("done: {n_written} new records in {}", out.display());
    Ok(())
}

fn percentile(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return f64::NAN;
    }
    let idx = ((sorted.len() - 1) as f64 * q).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

fn summarize(records: PathBuf, out: PathBuf) -> anyhow::Result<()> {
    let text = std::fs::read_to_string(&records)?;
    let mut recs: Vec<LiquidationRecord> = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        recs.push(serde_json::from_str(line)?);
    }
    anyhow::ensure!(!recs.is_empty(), "no records in {}", records.display());
    let n = recs.len() as f64;
    let mut by_protocol = BTreeMap::new();
    let mut by_liq: HashMap<&str, u64> = HashMap::new();
    let mut senders = std::collections::HashSet::new();
    let mut tips = Vec::new();
    let mut since = Vec::new();
    let (mut idx0, mut idx5, mut same, mut within2, mut contract) = (0u64, 0u64, 0u64, 0u64, 0u64);
    for r in &recs {
        *by_protocol.entry(r.protocol.clone()).or_insert(0) += 1;
        *by_liq.entry(r.liquidator.as_str()).or_insert(0) += 1;
        senders.insert(r.tx_from.as_str());
        let tip: u128 = r.priority_fee_wei.parse().unwrap_or(0);
        tips.push(tip as f64 / 1e9);
        if r.tx_index == 0 {
            idx0 += 1;
        }
        if r.tx_index < 5 {
            idx5 += 1;
        }
        if r.oracle_updates_same_block > 0 {
            same += 1;
        }
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
    let share = |k: usize| counts.iter().take(k).sum::<u64>() as f64 / n;
    tips.sort_by(|a, b| a.partial_cmp(b).unwrap());
    since.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let s = Summary {
        tool: format!("liqledger {}", env!("CARGO_PKG_VERSION")),
        first_block: recs.iter().map(|r| r.block).min().unwrap_or(0),
        last_block: recs.iter().map(|r| r.block).max().unwrap_or(0),
        n_liquidations: recs.len() as u64,
        by_protocol,
        unique_liquidators: by_liq.len() as u64,
        unique_tx_senders: senders.len() as u64,
        top1_share: share(1),
        top3_share: share(3),
        top10_share: share(10),
        median_priority_fee_gwei_total: percentile(&tips, 0.5),
        p90_priority_fee_gwei_total: percentile(&tips, 0.9),
        share_tx_index_0: idx0 as f64 / n,
        share_tx_index_lt_5: idx5 as f64 / n,
        share_same_block_oracle_update: same as f64 / n,
        share_oracle_update_within_2_blocks: within2 as f64 / n,
        median_blocks_since_oracle_update: if since.is_empty() { None } else { Some(percentile(&since, 0.5)) },
        contract_liquidator_share: contract as f64 / n,
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
        Cmd::Summarize { records, out } => summarize(records, out),
        Cmd::Topics => {
            for (name, h) in topics() {
                println!("{h:#x}  {name}");
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
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
    fn summary_shares_from_three_records() {
        let mk = |liq: &str, idx: u64, same: u64, since: Option<u64>| LiquidationRecord {
            protocol: "aave-v3".into(),
            block: 1,
            timestamp: 0,
            tx_hash: "0x".into(),
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
            priority_fee_wei: "1000000000".into(),
            builder: "0x0".into(),
            builder_extra_data: String::new(),
            oracle_updates_same_block: same,
            blocks_since_oracle_update: since,
        };
        let dir = std::env::temp_dir().join(format!("liqledger-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let recs = dir.join("r.jsonl");
        let lines: Vec<String> = [mk("0xa", 0, 1, Some(0)), mk("0xa", 3, 0, Some(2)), mk("0xb", 40, 0, None)]
            .iter()
            .map(|r| serde_json::to_string(r).unwrap())
            .collect();
        std::fs::write(&recs, lines.join("\n")).unwrap();
        let out = dir.join("s.json");
        summarize(recs, out.clone()).unwrap();
        let s: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(out).unwrap()).unwrap();
        assert_eq!(s["n_liquidations"], 3);
        assert!((s["top1_share"].as_f64().unwrap() - 2.0 / 3.0).abs() < 1e-9);
        assert!((s["share_tx_index_0"].as_f64().unwrap() - 1.0 / 3.0).abs() < 1e-9);
        assert!((s["share_same_block_oracle_update"].as_f64().unwrap() - 1.0 / 3.0).abs() < 1e-9);
        assert!((s["contract_liquidator_share"].as_f64().unwrap() - 1.0).abs() < 1e-9);
        assert_eq!(s["median_priority_fee_gwei_total"], 1.0);
    }
}
