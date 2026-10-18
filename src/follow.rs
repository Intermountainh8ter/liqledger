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
use serde::Serialize;
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
