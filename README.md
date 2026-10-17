# liqledger

**An open liquidation and MEV observatory for Ethereum mainnet.** Every Aave V3, Spark, and Morpho Blue liquidation the node still remembers, reconstructed from logs, receipts, and block headers alone: who won, where in the block it landed, what the winner paid the builder in priority fees, and how close it sat to a Chainlink oracle update. Pre-registered, bootstrapped, and honest about what a full node cannot see.

![language: Rust](https://img.shields.io/badge/language-Rust-blue) ![license: MIT](https://img.shields.io/badge/license-MIT-blue) ![data: own full node](https://img.shields.io/badge/data-own%20full%20node-informational)

## Why

Liquidations are the most contested transactions in DeFi, and almost everything written about who wins them and what they pay is anecdote or a vendor dashboard whose method is not published. This repository is the method. It needs nothing but a synced Geth full node: no archive, no traces, no indexer, no API key. Anyone with the same node can rerun it and get the same records.

The hypotheses were written down before the first full scan ([docs/PREREGISTRATION.md](docs/PREREGISTRATION.md)). The numbers below are rendered from committed result files, never typed.

## Results

<!-- RESULTS:START -->
Source: `results/summary.json`, blocks 22432941 to 26210252 (20930 liquidation events in 16510 transactions; aave-v3 14283, morpho-blue 5301, spark 1346). Shares carry 95% percentile bootstrap intervals, 2,000 resamples, seed 42.

| Pre-registered hypothesis | Measurement | Verdict |
|---|---|---|
| H1 concentration: top-3 liquidators win > 50% | top-3 share 0.300 [0.294, 0.306] | not supported: top-3 share 0.300 [0.294, 0.306] vs lower bound > 0.50 |
| H2 oracle adjacency: enrichment > 1.5x | same-block oracle update 0.608 [0.602, 0.615] vs baseline blocks 0.190 [0.172, 0.208]; enrichment 3.202 [2.930, 3.515] | supported: enrichment 3.20 [2.93, 3.52] vs lower bound > 1.5 |
| H3 block position: tx index < 5 in > 50% | share 0.313 [0.307, 0.319] | not supported: tx index < 5 share 0.313 [0.307, 0.319] vs lower bound > 0.50 |

| Descriptive | Value |
|---|---|
| Unique liquidator addresses / unique transaction senders | 787 / 902 |
| Top-1 / top-10 liquidator share | 0.196 / 0.541 |
| Liquidator is a contract (event liquidator != tx sender) | 0.968 |
| Priority fee paid per liquidation, median / p90 (ETH; lower bound, see below) | 0.00000 / 0.02939 |
| Priority fee exactly zero (builder paid by coinbase transfer instead) | 0.517 [0.510, 0.523] |
| Transaction index 0 | 0.052 |
| Oracle update within 2 blocks before (any feed) | 0.824 |
| Median blocks since last oracle update (any feed, 50-block lookback) | 0 |
| Builders, top 5 by liquidations included | Titan (titanbuilder.xyz) (10595); BuilderNet (Flashbots) (3266); BuilderNet (Nethermind) (1799); BuilderNet (Beaver) (1320); beaverbuild.org (1135) |
<!-- RESULTS:END -->

### What the numbers say

Two of my three pre-registered predictions were wrong, and the table says so.

- **The market is less concentrated than I expected.** 787 distinct liquidator addresses won at least one liquidation in 17 months; the top three took 30%, the top ten 54%. I predicted the top three would take more than half. They did not. The one-bot-takes-everything picture that circulates about liquidations does not describe mainnet Aave V3, Spark, and Morpho Blue over this window.
- **Liquidations ride the oracle.** 60.8% landed in the same block as a Chainlink price update, against 19.0% for blocks in general: a 3.2x enrichment with an interval well clear of the 1.5x threshold. 82.4% landed within two blocks of an update, and the median distance is zero. Whatever else winners do, they are reacting to the oracle transaction, not to the position drifting underwater.
- **Winners do not buy the top of the block.** Only 31.3% sat in the first five transaction slots and 5.2% at index zero. That is consistent with the previous point: the liquidation has to follow the oracle update inside the block, so it cannot be first.
- **Half the winners paid the builder off the books.** 51.7% of liquidations carried a priority fee of exactly zero, which on a builder-built block means the payment went by direct transfer, which this node cannot see. The median visible payment is therefore zero and the p90 is 0.029 ETH; both are lower bounds, as the pre-registration said they would be.
- **Almost everything is a contract, and half of it goes through Titan.** 96.8% of liquidations were executed by a contract rather than the signing wallet. Titan built 50.6% of the blocks containing a liquidation; the three BuilderNet operators together built another 30.5%.

Window: blocks 22,431,084 to 26,211,500 (2025-05-07 to 2026-10-17). 16,510 transactions produced 20,930 liquidation events; Aave V3 14,283, Morpho Blue 5,301, Spark 1,346.

## What a full node cannot show, said up front

- **Direct coinbase transfers.** Searchers commonly pay the builder with a transfer inside the transaction rather than a priority fee. Seeing that needs traces. The `priority_fee_wei` column is therefore a lower bound, and the share of liquidations with a priority fee of exactly zero is reported as the signature of that payment path, not as "paid nothing".
- **USD values.** Amounts are in raw token units. Pricing them needs per-block oracle answers, which the `AnswerUpdated` logs actually carry; that is the next version.
- **Which feed triggered which liquidation.** The oracle columns say "any Chainlink feed updated in this block or the 50 before"; attribution to the liquidated position's own collateral and debt feeds is deferred.
- **History floor.** Geth with history expiry serves blocks from roughly 17 months back. The window is whatever the node has, recorded in the summary.

## How it works

```
liqledger topics                                          # the three event signatures it filters on
liqledger scan --from 22431084 --out results/mainnet.jsonl  # one JSON record per liquidation, resumable
liqledger baseline --from 22431084 --to <head> --out results/baseline.json   # 2,000 random blocks: how often does ANY block carry an oracle update?
liqledger summarize --records results/mainnet.jsonl --baseline results/baseline.json --out results/summary.json
```

`scan` fetches `LiquidationCall` (Aave V3 and Spark) and `Liquidate` (Morpho Blue) logs in chunks, then for each one pulls the block header and the block's receipts in a single call (`eth_getBlockReceipts`, which works beyond the transaction-index horizon where receipt-by-hash does not). Priority fee is `(effective_gas_price - base_fee) * gas_used`. Oracle context is every Chainlink `AnswerUpdated` in the same block and in the 50 blocks before it, any feed.

`baseline` matters because many Chainlink feeds update often, so "the liquidation landed in a block with an oracle update" is meaningless without knowing how many blocks carry one anyway. The pre-registered test is the enrichment ratio between the two populations, with a bootstrap interval on each.

`summarize` reports every share with a 95% percentile bootstrap interval (2,000 resamples, seed 42) and prints a supported / not supported line for each pre-registered hypothesis. Fewer than 100 records means no verdict.

The RPC URL comes from `LIQLEDGER_RPC_URL` (default `http://127.0.0.1:8545`) and is never printed.

## What this is not

It observes past blocks. It submits nothing, holds no keys, and contains no trading logic. Nothing here is a signal.

## License

MIT.
