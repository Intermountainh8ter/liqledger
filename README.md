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

### v2: USD valuation and per-feed attribution (pre-registration addendum A: plain Chainlink proxies only)

Source: `results/summary-v2.json`. 314 of 20930 records priced from Chainlink answers at the record's block (Aave V3 and Spark only; Morpho Blue out of scope).

| Pre-registered hypothesis | Measurement | Verdict |
|---|---|---|
| H4 visible bids are small: median priority-fee share of gross profit < 0.50 | 0.848 [0.474, 0.936] (n with visible fee 144) | not supported: median visible bid share 0.848 [0.474, 0.936] vs upper bound < 0.50 (n=144) |
| H5 own-feed adjacency > 0.608 | 0.261 [0.213, 0.309] | not supported: own-feed same-block share 0.261 [0.213, 0.309] vs lower bound > 0.608 |
| H6 top-10% of liquidations carry > 80% of gross profit | 0.936 [0.765, 1.460] | not supported: top-10% profit share 0.936 [0.765, 1.460] vs lower bound > 0.80 |

| Descriptive | Value |
|---|---|
| Gross profit, total (USD, before gas and builder payment) | 164353 |
| Gross profit by protocol (USD) | aave-v3 164353 |
| Gross profit per liquidation, median / p90 (USD) | 31 / 1438 |
| Median blocks since the position's own feed last updated | 2 |
| Median visible bid share by gross-profit decile (D1 smallest) | D1 (n=14) 148.514; D2 (n=14) 1.839; D3 (n=15) 0.408; D4 (n=14) 0.323; D5 (n=15) 0.853; D6 (n=14) 0.962; D7 (n=14) 1.020; D8 (n=15) 0.886; D9 (n=14) 0.004; D10 (n=15) 0.076 |
| Median visible bid share by builder, top 5 | Titan (titanbuilder.xyz) (n=78) 0.756; BuilderNet (Flashbots) (n=19) 0.343; BuilderNet (Beaver) (n=14) 0.886; BuilderNet (Nethermind) (n=9) 0.881; beaverbuild.org (n=9) 0.882 |
| Unpriced records, by reason | morpho-blue: market oracle, not priced in v2: 5301; source has no aggregator(): custom adapter: 15315 |

Visible bid share counts only the priority fee; direct coinbase transfers are not in it, so every bid-share number here is a lower bound.

### v2: USD valuation and per-feed attribution (pre-registration addendum B: pricing through the protocols' adapter contracts)

Source: `results/summary-v2b.json`. 3182 of 20930 records priced from Chainlink answers at the record's block (Aave V3 and Spark only; Morpho Blue out of scope).

| Pre-registered hypothesis | Measurement | Verdict |
|---|---|---|
| H4 visible bids are small: median priority-fee share of gross profit < 0.50 | 0.726 [0.641, 0.815] (n with visible fee 1583) | not supported: median visible bid share 0.726 [0.641, 0.815] vs upper bound < 0.50 (n=1583) |
| H5 own-feed adjacency > 0.608 | 0.224 [0.209, 0.238] | not supported: own-feed same-block share 0.224 [0.209, 0.238] vs lower bound > 0.608 (n=3179) |
| H6 top-10% of liquidations carry > 80% of gross profit | 0.998 [0.935, 0.999] | supported: top-10% profit share 0.998 [0.935, 0.999] vs lower bound > 0.80 |

| Descriptive | Value |
|---|---|
| Gross profit, total (USD, before gas and builder payment) | 140175116 |
| Gross profit by protocol (USD) | aave-v3 138611912; spark 1563203 |
| Gross profit per liquidation, median / p90 (USD) | 14 / 1360 |
| Median blocks since the position's own feed last updated | 2 |
| Median visible bid share by gross-profit decile (D1 smallest) | D1 (n=158) 59.955; D2 (n=158) 0.294; D3 (n=158) 0.412; D4 (n=159) 0.594; D5 (n=158) 0.454; D6 (n=158) 0.936; D7 (n=159) 0.918; D8 (n=158) 0.891; D9 (n=158) 0.165; D10 (n=159) 0.019 |
| Median visible bid share by builder, top 5 | Titan (titanbuilder.xyz) (n=834) 0.836; BuilderNet (Flashbots) (n=242) 0.599; BuilderNet (Beaver) (n=124) 0.375; beaverbuild.org (n=120) 1.019; BuilderNet (Nethermind) (n=118) 0.575 |
| Unpriced records, by reason | adapter exposes no known components and is not constant: 143; component aggregator() reverted: 49; getPriceCap() failed: 388; morpho-blue: market oracle, not priced in v2: 5301; no answer at or before block: 11867 |
| Priced exactly / approximately (ratio-cap or aggregate on either side) | 1700 / 1482 |
| Source kinds (collateral/debt), top 8 | aggregate/fixed 1163; plain/fixed 496; plain/stable-cap 426; plain/plain 314; synchronicity/stable-cap 132; plain/synchronicity 107; ratio-cap/fixed 99; synchronicity/fixed 81 |
| Sensitivity, exact-priced records only (n=1700): H4 median visible bid share | 0.398 [0.273, 0.581] |
| Sensitivity, exact only: H5 own-feed same-block share | 0.266 [0.245, 0.287] |
| Sensitivity, exact only: H6 top-10% profit share | 0.893 [0.852, 0.924] |

Visible bid share counts only the priority fee; direct coinbase transfers are not in it, so every bid-share number here is a lower bound.

### v2: USD valuation and per-feed attribution (pre-registration addendum C: all aggregator phases)

Source: `results/summary-v2c.json`. 15030 of 20930 records priced from Chainlink answers at the record's block (Aave V3 and Spark only; Morpho Blue out of scope).

| Pre-registered hypothesis | Measurement | Verdict |
|---|---|---|
| H4 visible bids are small: median priority-fee share of gross profit < 0.50 | 0.432 [0.386, 0.499] (n with visible fee 6565) | supported: median visible bid share 0.432 [0.386, 0.499] vs upper bound < 0.50 (n=6565) |
| H5 own-feed adjacency > 0.608 | 0.284 [0.277, 0.291] | not supported: own-feed same-block share 0.284 [0.277, 0.291] vs lower bound > 0.608 (n=15027) |
| H6 top-10% of liquidations carry > 80% of gross profit | 0.984 [0.938, 0.994] | supported: top-10% profit share 0.984 [0.938, 0.994] vs lower bound > 0.80 |

| Descriptive | Value |
|---|---|
| Gross profit, total (USD, before gas and builder payment) | 190346279 |
| Gross profit by protocol (USD) | aave-v3 188783076; spark 1563203 |
| Gross profit per liquidation, median / p90 (USD) | 93 / 3399 |
| Median blocks since the position's own feed last updated | 1 |
| Median visible bid share by gross-profit decile (D1 smallest) | D1 (n=656) 12.189; D2 (n=657) 0.446; D3 (n=656) 0.669; D4 (n=657) 0.868; D5 (n=656) 0.808; D6 (n=657) 0.653; D7 (n=656) 0.229; D8 (n=657) 0.094; D9 (n=656) 0.030; D10 (n=657) 0.007 |
| Median visible bid share by builder, top 5 | Titan (titanbuilder.xyz) (n=3269) 0.580; BuilderNet (Flashbots) (n=1199) 0.406; BuilderNet (Nethermind) (n=671) 0.225; BuilderNet (Beaver) (n=556) 0.260; beaverbuild.org (n=400) 0.816 |
| Unpriced records, by reason | adapter exposes no known components and is not constant: 143; component aggregator() reverted: 49; constant over 100 blocks but described as a live price: 15; getPriceCap() failed: 392; morpho-blue: market oracle, not priced in v2: 5301 |
| Priced exactly / approximately (ratio-cap or aggregate on either side) | 12803 / 2227 |
| Source kinds (collateral/debt), top 8 | plain/stable-cap 8343; synchronicity/stable-cap 2264; aggregate/fixed 1163; ratio-cap/stable-cap 763; plain/fixed 494; fixed/stable-cap 456; plain/plain 314; stable-cap/plain 284 |
| Sensitivity, exact-priced records only (n=12803): H4 median visible bid share | 0.350 [0.313, 0.401] |
| Sensitivity, exact only: H5 own-feed same-block share | 0.297 [0.289, 0.305] |
| Sensitivity, exact only: H6 top-10% profit share | 0.935 [0.921, 0.948] |

Visible bid share counts only the priority fee; direct coinbase transfers are not in it, so every bid-share number here is a lower bound.

### v2: builder-payment bounds from the live follower (pre-registration addendum A.3)

Source: `results/summary-follow.json` (the follower's per-block records are kept outside the repository), blocks 26219854 to 26227080 (24.2 hours, 7227 blocks, 8 containing a liquidation; 2416 earlier rows written before the proposer-payment amendment are excluded, among them liquidation blocks 26219106, 26219648). `direct transfers` = coinbase balance delta + the builder's proposer payment - priority fees - withdrawals, per block; in a liquidation block it is an upper bound on what that liquidation paid the builder directly. Medians carry 95% percentile bootstrap intervals, 2,000 resamples, seed 42.

| Descriptive | Liquidation blocks | Other blocks |
|---|---|---|
| Blocks | 8 | 7219 |
| Direct transfers, median (ETH) | 0.0065 [0.0002, 0.0084] | 0.0002 [0.0001, 0.0002] |
| Direct transfers, p25 / p75 (ETH) | 0.0002 / 0.0071 | -0.0000 / 0.0014 |
| Direct transfers, p90 (ETH) | 0.0084 | 0.0052 |
| Direct transfers, mean (ETH) | 0.0049 | 0.0008 |
| Share of blocks with a negative figure (unidentified outflow) | 0.125 | 0.268 |
| Median difference, liquidation minus other (ETH) | 0.0063 [0.0000, 0.0083] | |

| Liquidation block | Builder | Direct transfers (ETH) | Priority fees (ETH) | Proposer payment (ETH) | Liquidations |
|---|---|---|---|---|---|
| 26220060 | Titan (titanbuilder.xyz) | 0.0065 | 0.0456 | 0.0247 | 1 |
| 26220794 | Titan (titanbuilder.xyz) | 0.0065 | 0.0143 | 0.0078 | 1 |
| 26221556 | BuilderNet | 0.0071 | 0.0222 | 0.0193 | 1 |
| 26221597 | Titan (titanbuilder.xyz) | 0.0084 | 0.0152 | 0.0078 | 1 |
| 26221599 | BuilderNet | 0.0121 | 0.0155 | 0.0094 | 1 |
| 26221640 | BuilderNet | -0.0017 | 0.0128 | 0.0068 | 1 |
| 26221679 | Titan (titanbuilder.xyz) | 0.0002 | 0.0097 | 0.0061 | 1 |
| 26224646 | Titan (titanbuilder.xyz) | 0.0002 | 0.0082 | 0.0036 | 1 |

Builders with most blocks in the window: Titan (titanbuilder.xyz) (3482 blocks, 5 with a liquidation, median 0.0007 ETH); ? Quasar (quasar.win) ? (1282 blocks, 0 with a liquidation, median -0.0001 ETH); BuilderNet (935 blocks, 3 with a liquidation, median 0.0002 ETH); Eureka (eurekabuilder.xyz) (589 blocks, 0 with a liquidation, median -0.0000 ETH); bombora.build ? (142 blocks, 0 with a liquidation, median -0.0024 ETH).

The proposer payment is identified heuristically as the block's last value-carrying transaction sent from the coinbase; a block where the builder pays differently, or spends from the coinbase for other reasons, shows up as a negative or understated figure. With a handful of liquidation blocks per day this table is descriptive and carries no verdict.
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

- **Direct coinbase transfers.** Searchers commonly pay the builder with a transfer inside the transaction rather than a priority fee. Seeing that needs traces. The `priority_fee_wei` column is therefore a lower bound, and the share of liquidations with a priority fee of exactly zero is reported as the signature of that payment path, not as "paid nothing". The live follower measures the direct path per block from coinbase balance changes; see "builder-payment bounds from the live follower" above.
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
