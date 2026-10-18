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

---

## Addendum A (2026-10-18, before any v2 computation): USD valuation, per-feed attribution, builder-payment bounds

Written after the v1 results above were published and before any v2 number exists. v1 hypotheses and
verdicts are unchanged.

### A.1 USD valuation (Aave V3 and Spark only)

- For each asset in a record, the protocol oracle's `getSourceOfAsset(asset)` is read at the head block
  (Aave V3 oracle `0x54586bE62E3c3580375aE3723C145253060Ca0C2`; Spark oracle as returned by Spark's
  PoolAddressesProvider, recorded in `results/feeds.json`). If the source exposes `aggregator()`, that
  aggregator's `AnswerUpdated` logs are the price history; otherwise the asset is **unpriced** (custom
  adapters, synchronicity oracles) and counted as such. Sources are read at head only, so feed changes
  during the window are a known error source and are reported, not corrected.
- Price at a block = the latest `AnswerUpdated` on that aggregator at or before the block, within the
  node's history. Records before the current aggregator's first log are unpriced.
- `seized_usd` = collateral seized × price; `repaid_usd` = debt repaid × price; `gross_profit_usd` =
  seized − repaid (the liquidation bonus before gas and builder payment). Token decimals via `decimals()`.
- `visible_bid_share` = priority fee (ETH, via the WETH feed at the block) ÷ gross_profit_usd. This is a
  lower bound on what the winner paid.
- Morpho Blue records are not priced in v2 (market-specific oracles).

### A.2 Per-feed oracle attribution

For each priced record: blocks since the collateral feed's and the debt feed's own last `AnswerUpdated`
(not "any feed"), and whether either own feed updated in the same block.

### A.3 Builder-payment bounds from a live follower

`liqledger follow` runs against the local node from its start time forward and records, per block: the
coinbase address, its balance delta from block N−1 to N, the sum of priority fees in the block, the
withdrawals credited to that address in the block, and `direct_transfers = delta − fees − withdrawals`.
For blocks containing a liquidation this is an **upper bound** on that liquidation's direct payment (other
transactions in the block may have paid too). Exact per-transaction attribution needs traces and is
deferred to the archive endpoint. Balance reads are only possible for recent blocks, which is why this is
a follower and not a backfill.

### A.4 Hypotheses fixed now (95% bootstrap intervals, seed 42, 2,000 resamples; ≥100 priced records)

- **H4 (visible bids are small).** Among priced Aave/Spark records with a non-zero priority fee, the
  median `visible_bid_share` is below 0.50. Supported if the interval's upper bound is below 0.50.
- **H5 (own-feed adjacency).** Among priced records, the share whose own collateral or debt feed updated
  in the same block exceeds the any-feed share measured in v1 (0.608). Supported if the lower bound
  exceeds 0.608.
- **H6 (bonus concentration).** The top 10% of priced liquidations by `gross_profit_usd` carry more than
  80% of total gross profit. Supported if the lower bound exceeds 0.80.

Descriptive: gross profit by protocol, pair and size decile; unpriced share and why; the clearing-price
curve (visible bid share by size decile and by builder); from the follower, the distribution of
`direct_transfers` in liquidation blocks versus non-liquidation blocks.

---

## Addendum B (2026-10-18, after the addendum A results, before any addendum B computation): pricing through Aave and Spark adapter contracts

Addendum A priced 314 of 20,930 records because most Aave V3 and Spark assets sit behind adapter
contracts that have no `aggregator()`. Probing those adapters at the head block (eth_call only) shows
they expose their components. This addendum fixes how each adapter kind is priced before any record is
priced that way. Addendum A's results stand as the first test of H4 to H6; this addendum re-tests the
same three hypotheses, with the same thresholds, on the enlarged sample, and reports both.

### B.1 Adapter kinds, detected by which getters answer at head

| Kind | Detected by | Price at block b | Exactness |
|---|---|---|---|
| plain proxy | `aggregator()` | latest `AnswerUpdated` of that aggregator at or before b (addendum A) | exact |
| stable cap (`PriceCapAdapterStable`) | `ASSET_TO_USD_AGGREGATOR()`, `getPriceCap()` | min(underlying aggregator's answer at or before b, cap read at head) | exact when uncapped; cap governance changes inside the window are an unmeasured error source |
| peg-to-base synchronicity (wBTC/BTC/USD) | `ASSET_TO_PEG()`, `PEG_TO_BASE()`, `DENOMINATOR()` | (asset-to-peg answer at or before b) × (peg-to-base answer at or before b), each from its own aggregator's logs | exact |
| ratio cap (`PriceCapAdapter`, wstETH, weETH, …) | `BASE_TO_USD_AGGREGATOR()`, `RATIO_PROVIDER()`, `getRatio()`, `getSnapshotRatio()`, `getSnapshotTimestamp()`, `RATIO_DECIMALS()` | (base answer at or before b) × ratio(t_b), where ratio(t) is linear in time through the two points the contract gives: (snapshot timestamp, snapshot ratio) and (head timestamp, head ratio), extrapolated with the same slope before the snapshot. The cap is not applied (`isCapped()` is false at head for every such adapter; applying it needs the ratio at b). | **approximate**: the exchange rate drifts a few percent a year, so the interpolation error over this window is a fraction of a percent; it is stated, not corrected |
| multi-source aggregate (Spark ETH/USD, BTC/USD from Chronicle, Chainlink, RedStone) | `chainlink()`, `chronicle()`, `redstone()` | the Chainlink component's answer at or before b | **approximate**: the adapter reports a median of three sources; the deviation from Chainlink alone is normally a few basis points |
| fixed | none of the above; `latestAnswer()` identical at head and at head − 100 blocks | that constant | exact if the adapter was the asset's source for the whole window; a mid-window switch from a live feed to a fixed value is an unmeasured error source |
| anything else (Pendle PT linear-discount adapters, MKR/USD calculated, unknown) | — | unpriced, counted by reason | — |

Every adapter is checked at head: the price composed by the rule above, evaluated at the head block,
must match the adapter's own `latestAnswer()` within 0.5%, or the adapter is left unpriced with reason
"head check failed". The relative error is recorded per adapter in `results/feeds.json`.

### B.2 Record fields added

`collateral_source_kind`, `debt_source_kind` (the kinds above), `approx` (true when either side is a
ratio-cap or multi-source aggregate). "Blocks since own feed" uses the underlying aggregator (stable cap),
the fresher of the two components (synchronicity), the Chainlink component (aggregate), the base
aggregator (ratio cap); a fixed side has no updates and does not count toward own-feed adjacency. A record
whose both sides are fixed is excluded from H5.

### B.3 What is tested

H4, H5, H6 exactly as in A.4, computed over all priced records (exact and approximate together), reported
as the addendum B verdicts next to the addendum A verdicts. One pre-specified sensitivity line: the same
three statistics over exact-priced records only. Coverage target, stated in advance: more than 80% of
Aave V3 and Spark records priced. Nothing else changes; Morpho Blue stays out of scope.
