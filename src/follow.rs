//! v2 (addendum A.3): live follower recording block-level builder-payment bounds.
//!
//! For each new block: coinbase, its balance delta from the previous block,
//! the sum of priority fees paid in the block, withdrawals credited to the
//! coinbase address in that block, and `direct_transfers = delta - fees -
//! withdrawals`. For a block that contains a liquidation this is an upper
//! bound on that liquidation's direct builder payment (other transactions may
//! have paid too). Per-transaction attribution needs traces and is deferred.
//! Balance reads exist only for recent blocks, which is why this runs forward
//! from its start and never backfills.

use crate::{LiquidationCall, Liquidate};
use alloy::eips::BlockId;
use alloy::primitives::{Address, U256};
use alloy::providers::{Provider, ProviderBuilder};
use alloy::rpc::types::BlockNumberOrTag;
use alloy::sol_types::SolEvent;
use serde::{Deserialize, Serialize};
use std::io::Write as _;
use std::path::PathBuf;

#[derive(Serialize)]
struct BlockPayment {
    block: u64,
    timestamp: u64,
    builder: String,
    builder_extra_data: String,
    tx_count: usize,
    base_fee_wei: String,
    /// Balance(coinbase, N) - Balance(coinbase, N-1), in wei (may be negative if the builder spent).
    coinbase_delta_wei: String,
    priority_fees_wei: String,
    withdrawals_to_coinbase_wei: String,
    /// The block's last transaction sent from the coinbase with value > 0 (the builder paying the
    /// proposer's fee recipient), added back so the delta reflects what the builder received.
    proposer_payment_wei: String,
    proposer_fee_recipient: Option<String>,
    /// delta + proposer payment - priority fees - withdrawals: direct payments (and any other inflow) net of other outflows.
    direct_transfers_wei: String,
    liquidation_txs: Vec<String>,
    n_liquidations: usize,
}

pub async fn follow(rpc: String, out: PathBuf, blocks: Option<u64>) -> anyhow::Result<()> {
    let provider = ProviderBuilder::new().connect_http(rpc.parse()?);
    let pools: Vec<Address> = vec![crate::AAVE_V3_POOL.parse()?, crate::SPARK_POOL.parse()?];
    let morpho: Address = crate::MORPHO_BLUE.parse()?;
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(&out)?;
    let mut last = provider.get_block_number().await?;
    let mut done = 0u64;
    eprintln!("following from block {}", last + 1);
    loop {
        if let Some(n) = blocks {
            if done >= n { break; }
        }
        let head = provider.get_block_number().await?;
        if head <= last {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            continue;
        }
        for bn in (last + 1)..=head {
            let Some(block) = provider.get_block_by_number(BlockNumberOrTag::Number(bn)).full().await? else { continue };
            let coinbase = block.header.beneficiary;
            let receipts = provider.get_block_receipts(BlockId::number(bn)).await?.unwrap_or_default();
            let base_fee = block.header.base_fee_per_gas.unwrap_or(0) as u128;
            let mut fees: U256 = U256::ZERO;
            let mut liq_txs = Vec::new();
            for r in &receipts {
                let tip = r.effective_gas_price.saturating_sub(base_fee).saturating_mul(r.gas_used as u128);
                fees += U256::from(tip);
                let is_liq = r.inner.logs().iter().any(|l| {
                    let t0 = l.topics().first().copied();
                    (pools.contains(&l.address()) && t0 == Some(LiquidationCall::SIGNATURE_HASH)) || (l.address() == morpho && t0 == Some(Liquidate::SIGNATURE_HASH))
                });
                if is_liq { liq_txs.push(format!("{:#x}", r.transaction_hash)); }
            }
            let withdrawals: U256 = block.withdrawals.as_ref().map(|ws| ws.iter().filter(|w| w.address == coinbase).map(|w| U256::from(w.amount) * U256::from(1_000_000_000u64)).fold(U256::ZERO, |a, b| a + b)).unwrap_or(U256::ZERO);
            let bal_now = provider.get_balance(coinbase).block_id(BlockId::number(bn)).await?;
            let bal_prev = provider.get_balance(coinbase).block_id(BlockId::number(bn - 1)).await?;
            let delta: i128 = i128::try_from(bal_now).unwrap_or(i128::MAX) - i128::try_from(bal_prev).unwrap_or(0);
            // A.3 amendment (2026-10-18): builders pay the proposer from the coinbase address in the
            // block's last transaction; that outflow is not a cost of any searcher and is added back.
            let (mut proposer_payment, mut fee_recipient): (U256, Option<Address>) = (U256::ZERO, None);
            if let Some(txs) = block.transactions.as_transactions() {
                for tx in txs.iter().rev() {
                    use alloy::consensus::Transaction as _;
                    if tx.inner.signer() == coinbase && tx.inner.value() > U256::ZERO {
                        proposer_payment = tx.inner.value();
                        fee_recipient = tx.inner.to();
                        break;
                    }
                }
            }
            let direct: i128 = delta + i128::try_from(proposer_payment).unwrap_or(0) - i128::try_from(fees).unwrap_or(0) - i128::try_from(withdrawals).unwrap_or(0);
            let rec = BlockPayment {
                block: bn,
                timestamp: block.header.timestamp,
                builder: format!("{coinbase:#x}"),
                builder_extra_data: String::from_utf8_lossy(&block.header.extra_data).chars().filter(|c| !c.is_control()).collect(),
                tx_count: receipts.len(),
                base_fee_wei: base_fee.to_string(),
                coinbase_delta_wei: delta.to_string(),
                priority_fees_wei: fees.to_string(),
                withdrawals_to_coinbase_wei: withdrawals.to_string(),
                proposer_payment_wei: proposer_payment.to_string(),
                proposer_fee_recipient: fee_recipient.map(|a| format!("{a:#x}")),
                direct_transfers_wei: direct.to_string(),
                n_liquidations: liq_txs.len(),
                liquidation_txs: liq_txs,
            };
            writeln!(file, "{}", serde_json::to_string(&rec)?)?;
            done += 1;
            if rec.n_liquidations > 0 || done % 50 == 0 {
                eprintln!("block {bn}: direct {:.4} ETH, fees {:.4} ETH, liquidations {}", direct as f64 / 1e18, u128::try_from(fees).unwrap_or(0) as f64 / 1e18, rec.n_liquidations);
            }
        }
        last = head;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// follow-summarize (A.4 descriptive item: direct transfers in liquidation blocks vs other blocks)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct FollowRow {
    block: u64,
    timestamp: u64,
    builder: String,
    #[serde(default)]
    builder_extra_data: String,
    priority_fees_wei: String,
    /// Absent in rows written before the A.3 amendment (no proposer-payment add-back); those rows are excluded.
    proposer_payment_wei: Option<String>,
    direct_transfers_wei: String,
    #[serde(default)]
    liquidation_txs: Vec<String>,
    n_liquidations: usize,
}

#[derive(Serialize)]
struct Dist {
    n: usize,
    mean_eth: f64,
    p10_eth: f64,
    p25_eth: f64,
    median_eth: f64,
    p75_eth: f64,
    p90_eth: f64,
    /// Share of blocks whose direct-transfer figure is negative (an outflow the add-back did not identify).
    share_negative: f64,
    median_ci: crate::Ci,
}

#[derive(Serialize)]
struct LiqBlock {
    block: u64,
    timestamp: u64,
    builder: String,
    direct_transfers_eth: f64,
    priority_fees_eth: f64,
    proposer_payment_eth: f64,
    n_liquidations: usize,
    liquidation_txs: Vec<String>,
}

#[derive(Serialize)]
struct FollowSummary {
    source: String,
    n_rows: usize,
    n_pre_amendment_excluded: usize,
    pre_amendment_liquidation_blocks: Vec<u64>,
    n_blocks: usize,
    first_block: u64,
    last_block: u64,
    first_timestamp: u64,
    last_timestamp: u64,
    span_hours: f64,
    n_liquidation_blocks: usize,
    n_liquidation_txs: usize,
    liquidation_blocks: Vec<LiqBlock>,
    direct_transfers_liquidation_blocks: Dist,
    direct_transfers_other_blocks: Dist,
    /// median(liquidation blocks) - median(other blocks), ETH; each group resampled independently.
    median_difference_eth: crate::Ci,
    /// (builder tag, blocks, liquidation blocks, median direct transfers ETH) for the five builders with most blocks.
    by_builder_top5: Vec<(String, usize, usize, f64)>,
}

fn wei_str_to_eth(s: &str) -> f64 {
    s.parse::<i128>().map(|w| w as f64 / 1e18).unwrap_or(f64::NAN)
}

fn dist(vals: &[f64], seed: u64) -> Dist {
    let mut sorted = vals.to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let median_ci = crate::bootstrap(
        vals.len(),
        |idx| {
            let mut v: Vec<f64> = idx.iter().map(|&i| vals[i]).collect();
            v.sort_by(|a, b| a.total_cmp(b));
            crate::percentile(&v, 0.5)
        },
        seed,
    );
    Dist {
        n: vals.len(),
        mean_eth: if vals.is_empty() { f64::NAN } else { vals.iter().sum::<f64>() / vals.len() as f64 },
        p10_eth: crate::percentile(&sorted, 0.10),
        p25_eth: crate::percentile(&sorted, 0.25),
        median_eth: crate::percentile(&sorted, 0.5),
        p75_eth: crate::percentile(&sorted, 0.75),
        p90_eth: crate::percentile(&sorted, 0.90),
        share_negative: if vals.is_empty() { f64::NAN } else { vals.iter().filter(|v| **v < 0.0).count() as f64 / vals.len() as f64 },
        median_ci,
    }
}

/// Summarise a follower file: the distribution of per-block direct transfers (coinbase delta plus
/// proposer payment minus priority fees minus withdrawals) in blocks that contain a liquidation
/// against all other blocks. Rows written before the proposer-payment amendment carry no
/// `proposer_payment_wei`; they are counted and excluded, never mixed in.
pub fn follow_summarize(records: PathBuf, out: PathBuf) -> anyhow::Result<()> {
    use rand::{Rng as _, SeedableRng as _};
    let text = std::fs::read_to_string(&records)?;
    let mut rows: Vec<FollowRow> = Vec::new();
    let mut n_rows = 0usize;
    let mut excluded = 0usize;
    let mut pre_liq = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() { continue; }
        let r: FollowRow = serde_json::from_str(line)?;
        n_rows += 1;
        if r.proposer_payment_wei.is_none() {
            excluded += 1;
            if r.n_liquidations > 0 { pre_liq.push(r.block); }
            continue;
        }
        rows.push(r);
    }
    anyhow::ensure!(!rows.is_empty(), "no amended follower rows in {}", records.display());
    rows.sort_by_key(|r| r.block);
    let direct: Vec<f64> = rows.iter().map(|r| wei_str_to_eth(&r.direct_transfers_wei)).collect();
    let liq: Vec<f64> = rows.iter().zip(&direct).filter(|(r, _)| r.n_liquidations > 0).map(|(_, d)| *d).collect();
    let other: Vec<f64> = rows.iter().zip(&direct).filter(|(r, _)| r.n_liquidations == 0).map(|(_, d)| *d).collect();
    let median_of = |v: &[f64]| { let mut s = v.to_vec(); s.sort_by(|a, b| a.total_cmp(b)); crate::percentile(&s, 0.5) };
    let point = median_of(&liq) - median_of(&other);
    let median_difference_eth = if liq.is_empty() {
        crate::Ci { point: f64::NAN, lo: f64::NAN, hi: f64::NAN }
    } else {
        let mut rng = rand_pcg::Pcg64Mcg::seed_from_u64(crate::BOOT_SEED);
        let mut vals: Vec<f64> = (0..crate::BOOT_ITERS)
            .map(|_| {
                let a: Vec<f64> = (0..liq.len()).map(|_| liq[rng.random_range(0..liq.len())]).collect();
                let b: Vec<f64> = (0..other.len()).map(|_| other[rng.random_range(0..other.len())]).collect();
                median_of(&a) - median_of(&b)
            })
            .collect();
        vals.sort_by(|a, b| a.total_cmp(b));
        crate::Ci { point, lo: crate::percentile(&vals, 0.025), hi: crate::percentile(&vals, 0.975) }
    };
    let liquidation_blocks: Vec<LiqBlock> = rows
        .iter()
        .zip(&direct)
        .filter(|(r, _)| r.n_liquidations > 0)
        .map(|(r, d)| LiqBlock {
            block: r.block,
            timestamp: r.timestamp,
            builder: if r.builder_extra_data.is_empty() { r.builder.clone() } else { r.builder_extra_data.clone() },
            direct_transfers_eth: *d,
            priority_fees_eth: wei_str_to_eth(&r.priority_fees_wei),
            proposer_payment_eth: r.proposer_payment_wei.as_deref().map(wei_str_to_eth).unwrap_or(f64::NAN),
            n_liquidations: r.n_liquidations,
            liquidation_txs: r.liquidation_txs.clone(),
        })
        .collect();
    let mut by_builder: std::collections::HashMap<String, (usize, usize, Vec<f64>)> = std::collections::HashMap::new();
    for (r, d) in rows.iter().zip(&direct) {
        let tag = if r.builder_extra_data.is_empty() { r.builder.clone() } else { r.builder_extra_data.clone() };
        let e = by_builder.entry(tag).or_insert((0, 0, Vec::new()));
        e.0 += 1;
        if r.n_liquidations > 0 { e.1 += 1; }
        e.2.push(*d);
    }
    let mut by_builder_top5: Vec<(String, usize, usize, f64)> = by_builder.into_iter().map(|(k, (n, l, v))| (k, n, l, median_of(&v))).collect();
    by_builder_top5.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    by_builder_top5.truncate(5);
    let first = &rows[0];
    let last = &rows[rows.len() - 1];
    let s = FollowSummary {
        source: records.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_else(|| records.display().to_string()),
        n_rows,
        n_pre_amendment_excluded: excluded,
        pre_amendment_liquidation_blocks: pre_liq,
        n_blocks: rows.len(),
        first_block: first.block,
        last_block: last.block,
        first_timestamp: first.timestamp,
        last_timestamp: last.timestamp,
        span_hours: (last.timestamp.saturating_sub(first.timestamp)) as f64 / 3600.0,
        n_liquidation_blocks: liq.len(),
        n_liquidation_txs: rows.iter().map(|r| r.n_liquidations).sum(),
        liquidation_blocks,
        direct_transfers_liquidation_blocks: dist(&liq, crate::BOOT_SEED),
        direct_transfers_other_blocks: dist(&other, crate::BOOT_SEED),
        median_difference_eth,
        by_builder_top5,
    };
    std::fs::write(&out, serde_json::to_string_pretty(&s)?)?;
    println!(
        "follow-summarize: {} amended blocks ({:.1} h, {} excluded pre-amendment), {} liquidation blocks; median direct transfers liq {:.4} ETH vs other {:.4} ETH",
        s.n_blocks, s.span_hours, s.n_pre_amendment_excluded, s.n_liquidation_blocks, s.direct_transfers_liquidation_blocks.median_eth, s.direct_transfers_other_blocks.median_eth
    );
    Ok(())
}
