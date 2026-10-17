# liqledger pre-registration

Written 2026-10-17 before the first full scan. Append-only: anything learned after the scan goes in
a dated addendum at the bottom, never in an edit above this line.

## 1. Question

On Ethereum mainnet, who wins lending-protocol liquidations, how concentrated is the winning, what
do winners pay the block builder in priority fees, and how closely do winning transactions follow
Chainlink oracle updates?

## 2. Data source and window

- One local Geth full node (snap sync, history expiry on). Everything comes from `eth_getLogs`,
  `eth_getTransactionReceipt` and `eth_getBlockByNumber`. No archive state, no traces, no third-party
  index or API.
- Window: block 22,431,084 (the earliest block the node still serves, 2025-05-07 10:05 UTC) to the
  head block at the moment the full scan starts. The head block number is recorded in the summary.
- Protocols: Aave V3 Pool `0x87870Bca3F3fD6335C3F4ce8392D69350B4fA4E2`, Spark Pool
  `0xC13e21B648A5Ee794902342038FF3aDAB66BE987`, Morpho Blue `0xBBBBBbbBBb9cC5e90e3b3Af64bdAF62C37EEFFCb`.
- Events (topic hashes printed by `liqledger topics`):
  `LiquidationCall` 0xe413a321…5286 (Aave V3 and Spark), `Liquidate` 0xa4946ede…7e41 (Morpho Blue),
  `AnswerUpdated` 0x0559884f…fc5f (every Chainlink aggregator).
- One record per liquidation event. A transaction that liquidates twice produces two records; the
  summary also reports unique transactions.
- Exclusions: none by design. A log that fails to decode is counted and reported, not dropped silently.

## 3. Measured per liquidation

protocol, block, timestamp, tx hash, tx index, block tx count, liquidator (event field), tx sender
(receipt `from`), borrower, collateral and debt asset (Morpho: market id), amounts repaid and seized
(raw units), gas used, effective gas price, base fee, priority fee paid
`(effective_gas_price - base_fee) * gas_used`, builder (coinbase) and its extra-data tag, number of
Chainlink `AnswerUpdated` events in the same block (any feed), and blocks since the most recent
`AnswerUpdated` within the previous 50 blocks (any feed).

## 4. Not measurable from this node, stated up front

- Direct coinbase transfers. Builders are commonly paid by a transfer inside the transaction, not only
  by priority fee. Those need traces or archive balances. `priority_fee_wei` is therefore a lower bound
  on what the winner paid, and no claim about "share of profit bid to builders" is made in this study.
- USD value of what was seized or repaid. Needs per-asset prices at the block. Deferred to a later
  version that reads the Chainlink answers already present in the `AnswerUpdated` logs.
- Which oracle feed triggered a given liquidation. The oracle columns are "any feed updated"; tying a
  liquidation to its own collateral and debt feeds is a later version.
- Anything on L2s. Mainnet only.

## 5. Hypotheses, fixed now

Shares are reported with 95% percentile bootstrap intervals (seeded, 2,000 resamples) over
liquidation records. A hypothesis is reported as supported only if its interval bound clears the
threshold; otherwise as not supported. No claim is made from fewer than 100 records.

- **H1, concentration.** The three most frequent `liquidator` addresses win more than half of all
  liquidations. Threshold: top-3 share lower bound > 0.50.
- **H2, oracle adjacency.** Liquidations land in the same block as a Chainlink update far more often
  than blocks in general contain one. Because many feeds update often, the raw share is meaningless
  alone, so the comparison is against a baseline: the share of 2,000 uniformly sampled blocks in the
  same window that contain at least one `AnswerUpdated` (`liqledger baseline`). Threshold: enrichment
  ratio (liquidation share / baseline share) lower bound > 1.5.
- **H3, block position.** Winners buy position. Threshold: share of liquidations with transaction
  index below 5 lower bound > 0.50.

Descriptive, no threshold: counts by protocol; unique liquidators and senders; contract-vs-EOA
liquidator share; priority-fee distribution (median, p90); share of liquidations with a priority fee
of exactly zero (the signature of a builder paid by coinbase transfer instead); builder distribution
by extra-data tag; distribution of blocks-since-oracle-update.

## 6. Reporting

- Records are committed as JSONL (`results/mainnet.jsonl`), the summary as JSON
  (`results/summary.json`), both with a SHA-256 manifest. The README table is rendered from the
  summary file, never typed.
- Every number in the README carries its interval. Null and contrary results are reported with the
  same prominence as supported ones.
- Nothing in this repository is a trading signal. The tool observes past blocks; it submits nothing.

## 7. Deferred, openly

Coinbase-transfer capture from a live follower (balances are available for the most recent blocks
only); USD valuation from oracle answers; per-liquidation feed attribution; L2 coverage via an
external RPC supplied by environment variable; a weekly automated refresh.
