## 2026-10-27T08:54:21Z defcheck proposal:13c4377d6738882414721f2ba72f519069cc9fd005c7554e6e1f732585d0c20f

Hypothesis: on 1000 seeded synthetic fixtures (seed 7) with a stated mix of payment channels, the gas-only
and all-channel share definitions differ by more than 0.05 on at least 300 fixtures, so a gas-only bid share
is not interchangeable with an all-channel share.

Fixture model (fixed before running): bonus B = 1 (normalised). Priority fee share ~ U[0, 0.5], always present.
Direct coinbase transfer present with probability 0.5, share ~ U[0, 0.6]. In-call tip present with probability
0.1, share ~ U[0, 0.2]. Protocol fee present with probability 0.3, share 0.1. Definitions: gas-only = priority fee / B;
all-channel = (priority + coinbase + in-call tip) / B; share-of-bonus = (all-channel + protocol fee) / B.
A fixture disagrees when |gas-only - all-channel| > 0.05.

Metric: /summary/definition_disagreement_fixture_count. Expected: at least 300 of 1000.
Falsifier: a count below 300 of 1000 under this mix and threshold.
Limit: the mix is an assumption, not data; the result describes the arithmetic of the definitions under that
mix and says nothing about real builders.

## 2026-10-27T08:59:44Z refundcheck proposal:6d637324f36e935002a8a7d5187ed66f51835469d40678e196c253191170d8cb

Hypothesis: on 1000 seeded synthetic fixtures (seed 7) where 30% carry a refund of uniform 0-50% of the
all-channel payment, the count of fixtures whose gross-of-refund and net-of-refund shares differ by more than 0.05
is at least 150 and at most 300. A refund is assumed visible in the same block in 20% of refunded fixtures; the rest
are not observable from same-block data, so a same-block-only measurement cannot net them.

Fixture model (fixed before running): payment channels drawn exactly as in defcheck (priority fee share ~ U[0, 0.5];
coinbase transfer with p 0.5, ~ U[0, 0.6]; in-call tip with p 0.1, ~ U[0, 0.2]). Gross share = all-channel payment P.
Refund present with p 0.3, fraction r ~ U[0, 0.5] of P; net = P * (1 - r). Visible with p 0.2 given a refund.
A fixture disagrees when |gross - net| > 0.05.

Metric: /summary/refund_gross_net_disagreement_count. Expected: between 150 and 300 of 1000.
Falsifier: a count below 150 or above 300 under seed 7 and this mix. Secondary (descriptive, not pre-registered):
the split of disagreeing fixtures into visible and not-visible refunds.
Limit: the mix is an assumption, not data; refunds are not attributable on-chain per the cited source, so this
describes arithmetic only.

## 2026-10-27T09:04:09Z dustcheck proposal:7d5a07a2987d8525818f7b953ac97f6e9f10691111e6eacebff8b6f90977e9be

Hypothesis: on 1000 seeded synthetic fixtures (seed 7) with debt drawn from a fixed assumed range, at least 5% of
fixtures have a repaid amount that differs between the target-health-factor rule alone and the same rule plus the
remaining-debt dust rule, so a local re-execution that omits the dust rule diverges from a landed liquidation.

Fixture model (fixed before running): debt D ~ U[500, 10000] USD in the target reserve. The target-health-factor rule
repays a fraction f ~ U[0.05, 0.6] of D (R = f * D). The dust rule, as documented for Aave V4: if remaining debt
D - R is below 1000 USD, the full debt D is repaid. A fixture diverges when the two repaid amounts differ.

Metric: /summary/dust_repay_divergence_count. Expected: at least 50 of 1000 (range 50 to 1000).
Falsifier: a count below 50 of 1000 under seed 7 and this mix.
Limit: the ranges are assumptions, not data; the page cited does not mention collateral, so this models debt only.
It describes the arithmetic of the rule, not how often real liquidations hit it.

## 2026-10-27T09:14:40Z attribcheck proposal:d7859a9500180f9a068880b14847923effd861ff6d36ea47aa25f5e963d56242

Hypothesis: on 1000 seeded synthetic blocks (seed 7), each holding an adjacent oracle update and liquidation, half of
which share a private origin and half coincidental, a public-only same-block adjacency rule labels every adjacent pair
as origin-shared, so at least 400 of 1000 blocks are misclassified.

Fixture model (fixed before running): every block has an oracle update and a liquidation in the same block. Shared
origin with p 0.5. Origin is not observable from public data, so the rule sees only adjacency. A block is
misclassified when the rule says shared and the fixture is coincidental.

Metric: /summary/adjacency_misclassified_block_count. Expected: between 400 and 600 of 1000.
Falsifier: a count below 400 of 1000 under seed 7, meaning adjacency alone separates the two kinds on the fixtures.
Limit: the 0.5 mix and the all-adjacent design are assumptions, so the count restates the mix; it is not a measured
rate of shared origin on chain.

## 2026-10-27T09:19:15Z fallbackcheck proposal:437bea6e0214f92d5009a5c2ddf597032215a36cd6593e7d6f322843d233993a

Hypothesis: in a seeded synthetic model of 1000 liquidations (seed 7) where half of oracle updates take the fallback
path, a public-only adjacency rule misattributes strictly more liquidations at a 60 s fallback delay than at a 10 s
delay, because a longer delay widens the window in which unrelated updates can land adjacent.

Fixture model (fixed before running): each liquidation follows one true update; the path is fallback with p 0.5, else
private. Only fallback-path liquidations are exposed to the delay D (10 s or 60 s). During D, unrelated updates arrive
as a Poisson count with mean D / 60, each private or fallback with p 0.5. The public-only rule picks one candidate
uniformly among the true update and the unrelated ones and attributes the liquidation to that candidate's path. A
liquidation is misattributed when the picked candidate's path differs from the true path (fallback). Both delay
settings use the same seed and separate draws per setting.

Metric: /summary/fallback_path_misattributed_count (60 s setting); the 10 s count is reported alongside as
/summary/fallback_path_misattributed_count_10s. Expected: at least 1 and above the 10 s count (range 1 to 1000).
Falsifier: the 60 s count is less than or equal to the 10 s count at seed 7, or the count is zero.
Limit: the mix, arrival rate and uniform pick are assumptions, not data; no chain data is used.

## 2026-10-27T09:28:26Z bonuscheck proposal:fe45c91415d532c56df44213e15367f70a1b7c42e0d7536f2aa66dc827af40c8

Hypothesis: on 1000 seeded synthetic positions (seed 7) with a health-factor-dependent liquidation bonus, a repay
amount sized with a fixed bonus misses the target health factor by more than the 0.01 tolerance in at least 300 of
1000 fixtures.

Fixture model (fixed before running): debt D = 1; pre-liquidation health factor HF uniform on [0.90, 0.99]; liquidation
threshold LT uniform on [0.75, 0.90]; collateral C = HF * D / LT. Target health factor T = 1.05. True bonus
b(HF) = 0.05 + 0.05 * (1 - HF) / 0.10, so lower HF gives a higher bonus (5 percent at HF 1.0 up to 10 percent at HF 0.9).
The sizing rule assumes a fixed bonus b0 = 0.05 and repays x = (T*D - C*LT) / (T - (1 + b0)*LT), capped at D. The
post-liquidation health factor is computed with the true bonus: HF' = (C - x*(1 + b)) * LT / (D - x), and is set to
infinity if the debt is fully repaid. A fixture is a miss when |HF' - T| > 0.01 (a fully repaid position counts as a miss).

Metric: /summary/fixed_bonus_target_miss_count. Expected: at least 300 of 1000 (range 300 to 1000).
Falsifier: a count below 300 of 1000 under seed 7.
Limit: the ranges, linear bonus curve and target are assumptions, not the Aave V4 parameters; no chain data is used.

## 2026-10-27T09:35:18Z fractioncheck proposal:532c763c1a719db8b4fea961d7a60569b930945d121f9a05520805eba7dfdd60

Hypothesis: on 1000 seeded synthetic positions (seed 7) with a per-Spoke target health factor drawn uniformly from
{1.035, 1.044, 1.24}, a fixed 50 percent debt repay with a fixed 5 percent bonus leaves the post-liquidation health
factor below target by more than 0.01 in at least 300 of 1000 fixtures.

Fixture model (fixed before running): debt D = 1; pre-liquidation health factor HF uniform on [0.90, 0.99]; liquidation
threshold LT uniform on [0.75, 0.90]; collateral C = HF * D / LT. Repay x = 0.5 * D with bonus 0.05 (not HF-dependent,
so the shortfall is not a bonuscheck effect; the post-repay debt is 0.5 so dust is not involved). Post-liquidation
HF' = (C - x*1.05) * LT / (D - x). A fixture counts when HF' < target - 0.01.

Metric: /summary/fixed_fraction_below_target_count. Expected: at least 300 of 1000 (range 300 to 1000).
Falsifier: a count below 300 of 1000 under seed 7 and n 1000.
Limit: the ranges, target set and repay fraction are assumptions, not Aave V4 parameters; no chain data is used.

## 2026-11-11T10:26:24Z closefactor proposal:dbdbce8ab12810423b5f94ddbfa1ee51fcf1dd120b041da5cb567e6d7ea8817d

Hypothesis: on 1000 seeded synthetic positions (seed 7; debt 1, HF uniform on [0.90, 0.99], LT uniform on [0.75, 0.90],
bonus 0.05, target HF 1.05), three close-factor models disagree on repaid fraction (max minus min above 0.01) in at
least 900 of 1000 fixtures.

Models (fixed before running): A fixed 50 percent. B piecewise: 50 percent when HF >= 0.95, else 100 percent. C target
health factor: repay x = (T*D - C*LT) / (T - (1 + b)*LT) capped at D and floored at 0, and 100 percent when collateral
over debt C/D < 1/(1 + b) (the bad-debt threshold). Collateral C = HF * D / LT.

Metric: /disagreement/count_models_differ. Expected: at least 900 of 1000 (range 900 to 1000).
Falsifier: a count below 900 of 1000 under seed 7 and n 1000.
Limit: ranges, the piecewise cut and the target are assumptions, not Aave V4 parameters; no chain data is used.

## 2026-11-11T10:50:15Z denomshare proposal:1f2b69abf46cb9f4b9804ee70865cf679f4f2ad09470b415c7088c3cb7ba7bc3

Hypothesis: in a synthetic auction (seed 7, n 10000 liquidations, 5 builders) where builders differ in executed gas cost
and in the fraction of the bid paid as a visible priority fee, at least one of the 10 builder pairs swaps rank between
the gross-bonus denominator and the visible-bid denominator.

Model (fixed before running): each liquidation has gross bonus G uniform on [1, 100] and is won by one of 5 builders
uniformly. Builder b has gas fraction g_b uniform on [0.05, 0.60] and visible fraction f_b uniform on [0.2, 1.0], both drawn
once from the seed. Post-execution value P = G*(1-g_b); the bid is beta*P with beta uniform on [0.5, 1.0]; the visible
priority fee is f_b * bid. Per builder, the bid total is divided by three denominators (sum of G, sum of P, sum of visible
fee). Rankings by each ratio are compared pairwise; a flip is a pair ordered differently under gross vs visible.

Metric: /rank_flip_count. Expected: at least 1 of 10 (range 1 to 10).
Falsifier: rank_flip_count equals 0 under seed 7 and n 10000.
Limit: all parameter ranges are assumptions; no chain data is used.

## 2026-11-11T11:16:55Z samebloc proposal:840048e52971f5cb40621d05abd0a50fba6b75cd4b2c5f7ff145bbb0a342b496

Hypothesis: on a seeded synthetic ledger (seed 7, n 10000 blocks) with oracle updates and one liquidation per block at
uniformly random tx indices, counting only oracle updates earlier in tx order yields a strictly smaller same-block
count than counting any update in the block, so the spread between the two rules is positive.

Model (fixed before running): each block has 150 tx slots and one liquidation at a uniform slot. The number of oracle
updates in the block is 0, 1 or 2 with probability 0.4, 0.4, 0.2, each at a uniform slot distinct from the liquidation
slot. Rule A: any update in the block. Rule B: at least one update at a lower tx index than the liquidation. Rule C: the
latest update in the block is at a lower tx index than the liquidation.

Metric: /spread_any_vs_earlier (count under A minus count under B). Expected: greater than 0 (range 1 to 100000).
Falsifier: spread_any_vs_earlier equals 0 or is negative under seed 7 and n 10000.
Limit: slot count and update-count mix are assumptions; no chain data is used.

## 2026-11-11T11:25:24Z remaindercheck proposal:44ea371665e1f6522e7d7f84bb0786a1dfe18b4b49a7f0526b23593a95fe86bc

Hypothesis: on a seeded synthetic ledger (seed 7, n 10000 bids, 5 builders) where each bid has a nonzero unaccounted
remainder, ranking builders by recapture rate under the named-payee-sum denominator differs from the ranking under the
total-bid denominator in at least one of the 10 builder pairs.

Model (fixed before running): each bid is won by one of 5 builders uniformly, with bid size uniform on [1, 100]. Builder b
has a recaptured fraction c_b uniform on [0.1, 0.9] and an unaccounted fraction u_b uniform on [0.05, 0.5], both drawn
once from the seed. Recaptured amount is c_b times the bid; the named-payee sum is (1 - u_b) times the bid. Recapture rate
is recaptured amount over the denominator, summed per builder. A flip is a pair ordered differently under the two denominators.

Metric: /remainder/rank_flip_count. Expected: at least 1 of 10 (range 1 to 10).
Falsifier: rank_flip_count equals 0 under seed 7 and n 10000.
Limit: all ranges are assumptions; no chain data is used.

## 2026-11-11T11:43:58Z flashlock proposal:9ba728069773e2ff1824ac40d4f35edd5c836d1e69b80ee6b84c87ce5e9ae1ad

Hypothesis: on a seeded synthetic set of blocks (seed 7, n 10000 blocks) with competing liquidation bids carrying arrival
times and fees, ordering by sequential locked slots changes the rank of at least one bid in more than half of the blocks,
and changes which bid lands first in a non-zero count of blocks, relative to whole-block fee order.

Model (fixed before running): each block has 2 to 5 competing bids (uniform), each with an arrival time uniform on [0, 1)
of the block window and a fee uniform on [1, 100]. Whole-block order ranks bids by fee, highest first. Locked-slot order
splits the window into 10 equal slots; a bid lands in the first slot ending after its arrival, slots are ordered by time
and cannot be reordered once closed, and bids inside a slot are ordered by fee. A bid is rank-changed if its position differs.

Metric: /ordering/rank_changed_bid_count. Expected: greater than 0 (range 1 to 100000), together with a non-zero
first-lander-changed block count and rank-changed blocks above half of n.
Falsifier: rank_changed_bid_count equals 0 under seed 7 and n 10000.
Limit: slot count, bid count and distributions are assumptions; no chain data is used.

## 2026-11-11T11:51:00Z replaygap proposal:4b81bd32b963fed52e020a5ad22ba872c1b644e148b46f2ee593c3c4e633769c

Hypothesis: in a seeded toy re-execution of synthetic liquidations (seed 7, n 10000), perturbing only same-block
ordering yields a nonzero mismatch count, and ordering-only mismatches are fewer than state-only mismatches.

Model (fixed before running): each liquidation has a pre-block health factor uniform on [0.95, 1.0) and succeeds when
it is below 1. State perturbation multiplies the health factor by one plus a shock uniform on [-0.03, 0.03]. Each
landed transaction has a gas need uniform on [150000, 450000] and a limit of need times a slack uniform on [1.0, 1.3];
gas perturbation scales the limit by a factor uniform on [0.8, 1.2] and fails when the limit is below the need. Each
liquidation has a rival same-block liquidation with probability 0.1; ordering perturbation puts the rival first, and
the original fails when the rival's repaid share, uniform on [0, 1), exceeds 0.5. A mismatch is a changed outcome with
one input perturbed and the others held at landed values.

Metric: /mismatch/by_input/ordering_only_count. Expected: greater than 0 and below state_only_count.
Falsifier: ordering_only_count equals 0, or is at least state_only_count, on seed 7.
Limit: every range is an assumption; no chain data is used.

## 2026-11-11T12:10:00Z replayverify proposal:b690c39b25c11b315c5d408ea6ac681ec5977678eeab98e7238c6e68865c2c13

Hypothesis: on seeded synthetic replays with a one-input perturbation (seed 7, n 10000), a tolerance-based grader passes
at least 100 replays that an exact integer verifier fails, so the verdict flip count is at least 100 of 10000.

Model (fixed before running): each landed liquidation repays an integer amount uniform on [1000000, 1000000000000).
With probability 0.3 the replay is perturbed: its integer output differs from the landed one by the amount times
10 to the power u, rounded to an integer, where u is uniform on [-9, -3]. The exact verifier fails any replay whose
integer output differs. The tolerance grader passes a replay whose relative difference is at most 0.000001. A flip is a
replay the exact verifier fails and the tolerance grader passes.

Metric: /verdict_flip_count. Expected: at least 100 (range 100 to 10000).
Falsifier: verdict_flip_count below 100 on seed 7 with n 10000.
Limit: every range and the tolerance are assumptions; no chain data is used.

## 2026-11-11T12:14:15Z replaycoverage proposal:6bd5f14e0520ca0dced55b96eadda06555f67a4dd03dda8c687116a469b9670e

Hypothesis: a verdict log with seeded omissions and no orphan entries passes a per-verdict recomputation check yet
leaves a nonzero number of committed replays uncovered (seed 7, n 10000), so a coverage count finds omissions the
recompute check cannot.

Model (fixed before running): a committed list holds n replay ids. Each id is omitted from the verdict log with
probability 0.05. Each retained verdict records an integer output and a recomputed integer output that are equal. The
recompute check counts retained verdicts whose two outputs differ. The coverage check counts committed ids with no
verdict.

Metric: /uncovered_count. Expected: greater than 0 (range 1 to 10000) while the count of verdicts failing recomputation
stays 0.
Falsifier: uncovered_count equals 0 on seed 7, or the recompute check also flags the omissions.
Limit: the omission rate is an assumption; no chain data is used.

## 2026-11-11T12:18:19Z targethf proposal:4997dff3f58a95c325f7c2c4c91b01d8a8cf40c4c3c2f57d21590cc0830371d2

Hypothesis: on seeded toy positions (seed 7, n 1000), sizing each repayment to restore health factor to 1.0 leaves at
least 500 rows below a per-row target health factor above 1.0 after integer rounding, while sizing to the per-row target
leaves zero rows below it.

Model (fixed before running): each position has collateral uniform on [1000000, 1000000000000) integer units, a
liquidation threshold of 0.80 and a seizure bonus of 5 percent. Its starting health factor is uniform on [0.90, 1.00).
The per-row target is uniform on [1.01, 1.05]. Repay amounts are the smallest integers reaching the sizing target,
computed in integer arithmetic. Off target means post-liquidation health factor strictly below the per-row target.

Metric: /off_target_count (mismatched reading). Expected: at least 500 (range 500 to 1000).
Falsifier: off_target_count below 500 on seed 7 with n 1000, or the matched reading leaves any row below its target.
Limit: every range is an assumption; no chain data is used.

## 2026-11-11T12:22:30Z bonuscap proposal:741a0dcc848f122fd813f0daf294440fbf2a0857d5a58070584c4f7e2dd95a09

Hypothesis: when the true bonus saturates at a ceiling, repay sizing that assumes the unsaturated ramp misses the
target health factor by more than the tolerance only on rows below the saturation cutoff, and misses none above it.

Model (fixed before running): the assumed bonus is the bonuscheck ramp, 0.05 + 0.05 * (1 - hf) / 0.10. The true bonus is
that ramp capped at 0.07, which saturates at health factor 0.96 (the cutoff). Starting health factor is uniform on
[0.90, 0.99) and liquidation threshold uniform on [0.75, 0.90). Target health factor 1.05, tolerance 0.01. A row is a
miss when the post-liquidation health factor under the true bonus differs from the target by more than the tolerance.
Rows are split by whether starting health factor is below the cutoff.

Metric: /summary/below_target_count (below-cutoff misses). Expected: at least 1 (range 1 to 1000) and the above-cutoff
miss count equals 0.
Falsifier: a zero below-cutoff miss count, or any miss above the cutoff.
Limit: the ceiling and ranges are assumptions; no chain data is used.

## 2026-11-11T12:26:45Z bonusscale proposal:b158130cfa766e62d66f248da65fc8a1879870fd0fe07dbe7b3a81a4b47adb69

Hypothesis: on seeded synthetic positions in integer bps across two toy Spoke profiles with different bonus scale
factors and maximum bonuses, a ledger that applies one shared scale factor misprices the bonus on at least one row in
each profile, and the mispriced count is zero only when both profiles share the same scale.

Model (fixed before running): bonus bps = min(max, base + scale * (10000 - hf_bps) / 100), integer division, hf_bps
uniform on [9000, 10000). Volatile profile: base 500, scale 100, max 1000. Correlated profile: base 200, scale 25,
max 400. Each row is assigned to a profile by a fair coin. The shared-scale ledger uses scale 60 for both profiles and
keeps each profile's own base and max. A row is mispriced when the integer bonus differs. Control: both profiles with
scale 60 as the true scale give zero mispriced rows by construction.

Metric: /misprice_count. Expected: at least 1 (range 1 to 100000), with a nonzero count in each profile.
Falsifier: misprice_count equal to zero when the two toy profiles carry different scale factors.
Limit: every parameter is an assumption, not an Aave V4 value; no chain data is used.

## 2026-11-12T12:45:30Z premiumround proposal:d6da9a43ee2fd5ee72f4c90aeffae81c8b6523e60635f04a79b7da83642fb423

Hypothesis: on seeded synthetic integer positions whose debt sits within 4 wei of the liquidation boundary, a ledger
that adds up to 2 wei of rounded premium debt per accrual flips the liquidatable flag on at least one row versus the
exact integer recompute, and flips none when the boundary offset exceeds 2 wei times the accrual count.

Model (fixed before running): a row is liquidatable when debt exceeds a limit, both integers in wei. Band rows have
offset = debt minus limit uniform on [-4, 4] and k accruals uniform on [1, 3]. The rounded ledger adds an independent
uniform integer in [0, 2] wei per accrual. Control rows have offset = -(2k + 1 + u) with u uniform on [0, 3], so the
offset lies outside the band. A disagreement is a row where the two flags differ.

Metric: /disagreements (band rows). Expected: at least 1 (range 1 to 100000) and /control_disagreements equals 0.
Falsifier: a zero disagreement count on band rows, or any disagreement on control rows.
Limit: every parameter is an assumption, not an Aave V4 value; no chain data is used.

## 2026-11-12T12:57:10Z dustdeficit proposal:3550dd08ae2ad983aaac4360d928b698921fcebf7dbe7f0c6ca6d7cc20fb0acf

Hypothesis: on seeded synthetic integer positions within a few units of a dust threshold, a rounding-up and a
rounding-down integer recompute of the dust and deficit flags disagree on a nonzero count of positions.

Model (fixed before running): dust threshold 1000 units. Repay = debt * 5000 / 10000, rounded down (floor) or up (ceil);
remaining debt = debt - repay. Dust flag: remaining debt below 1000. Deficit flag: remaining debt above 0 and collateral
left equal to 0, where collateral left = collateral - seized and seized = collateral * repay / debt (floor or ceil with
the same direction, collateral = debt + uniform integer in [0, 2]). Debt uniform on [1996, 2004]. A disagreement is a row
where either flag differs between the two directions.

Metric: /disagree_count. Expected: at least 1 (range 1 to 100000).
Falsifier: disagree_count equal to 0 at the default sample size.
Limit: every parameter is an assumption, not an Aave V4 value; no chain data is used.

## 2026-11-12T13:01:22Z oracleadjacent proposal:e5fb5c30538abb1993c33d9c96098974fcd0415f0119a0c6611e418bc1484ed1

Hypothesis: in a seeded synthetic block model where 40% of liquidations are triggered by a same-block oracle update and
30% are independently mempool-visible, the count of liquidations landing within 3 transaction indices after the update
exceeds 60% of all oracle-triggered liquidations, and classifying by adjacency alone attributes at least 25% of
mempool-only liquidations to the oracle.

Model (fixed before running): each row has an oracle-triggered flag (probability 0.4) and an independent mempool-visible
flag (probability 0.3). The oracle update sits at a transaction index uniform on [0, 150). Oracle-triggered rows land
d indices after the update with d uniform on [1, 4]. Mempool-only rows (visible, not oracle-triggered) backrun the update
with probability 0.3 at d uniform on [1, 3], otherwise land at an index uniform on [0, 150). Remaining rows land at an
index uniform on [0, 150). A row is adjacent when 1 <= d <= 3 where d = landing index minus update index.

Metrics: /adjacent_count (primary; all rows adjacent), /oracle_adjacent_share (adjacent among oracle-triggered),
/mempool_only_adjacent_share (adjacent among mempool-only rows). Expected: adjacent_count at least 1000 (range 1000 to
4000), oracle_adjacent_share above 0.60, mempool_only_adjacent_share at least 0.25.
Falsifier: adjacent_count below 1000 at seed 7 and n 10000, or mempool_only_adjacent_share below 0.25.
Limit: every parameter is an assumption, not a measured value; no chain data is used.

## 2026-11-12T13:15:44Z stalelookback proposal:713304b8f18d31e405c245d88388d73016c5d0fdb8480fdf9f4c3e273a3e6fa2

Hypothesis: in a seeded synthetic model of 10000 landed liquidations with health factor uniform on 0.95 to 1.0 and a
per-block drift of standard deviation 0.003, the flag-flip count at lookback 8 blocks exceeds the count at lookback 1
block, and the count at lookback 0 is exactly zero.

Model (fixed before running): each row draws a landed health factor h uniform on [0.95, 1.0), so the liquidatable flag
(h < 1) is true at lookback 0. The pre-state k blocks stale is h plus the sum of k independent Gaussian steps with
standard deviation 0.003 (Box-Muller). A flip is a row whose flag is false in the stale pre-state (health factor at or
above 1). One walk per row is reused for every lookback 0 to 8.

Metric: /flag_flip_count_lookback_8. Expected: greater than /flag_flip_count_lookback_1, with lookback 0 equal to 0
(range 100 to 10000).
Falsifier: flag_flip_count_lookback_8 not greater than flag_flip_count_lookback_1, or lookback 0 count nonzero.
Limit: every parameter is an assumption, not a measured value; no chain data is used.

## 2026-11-13T13:34:09Z stalestate proposal:21b9a671e80a7c95dd466156d544399c859a213b3bf7c3fb565d01286cfe9069

Hypothesis: in a seeded synthetic model of 10000 landed liquidations (health factor uniform on 0.95 to 1.0; omitted oracle
update present in 60% of rows with shift of absolute value of a Gaussian with standard deviation 0.01; omitted earlier
same-block transaction present in 30% of rows with shift of absolute value of a Gaussian with standard deviation 0.005),
the count of flag flips attributable to the oracle input alone exceeds the count attributable to the earlier-transaction
input alone, and with neither input omitted the flip count is exactly zero.

Metric: /stalestate/oracle_only_count. Expected: greater than /stalestate/tx_only_count, with the no-omission flip count
equal to 0 (range 100 to 10000).
Falsifier: oracle_only_count not greater than tx_only_count, or a nonzero flip count with no omitted input.
Limit: every parameter is an assumption, not a measured value; no chain data is used. Shifts are applied upward to the
stale-state health factor; a flip is a stale health factor at or above 1.

## 2026-11-13T13:38:30Z stalestate2 proposal:fc3885542ac1ca28edc6bcaa2e6102ef6d730f8fb2e79e74ee24ccd09d6ac033

Hypothesis: in a seeded synthetic model of 10000 landed liquidations (health factor uniform on 0.95 to 1.0; omitted
oracle update in 60% of rows, shift absolute value of a Gaussian with standard deviation 0.01; omitted earlier
transaction in 30% of rows, standard deviation 0.005; omitted balance change in 10% of rows, standard deviation 0.008),
the count of flips attributable to the earlier-transaction input alone exceeds the count attributable to the
balance-change input alone, and with no input omitted the flip count is exactly zero.

Metric: /stalestate2/tx_only_count. Expected: greater than /stalestate2/balance_only_count, with the no-omission flip
count equal to 0 (range 100 to 10000).
Falsifier: tx_only_count not greater than balance_only_count, or a nonzero flip count with no omitted input.
Limit: every parameter is an assumption, not a measured value; no chain data is used. Shifts are applied upward to the
stale-state health factor; a flip is a stale health factor at or above 1. A row counts toward an input's alone count
when only that input's shift alone flips the flag; rows needing two or more inputs go to a multi-input count.

## 2026-11-13T13:46:30Z stalestate3 proposal:11521c893dae6b385f198e8c62759608c64b931bdd3c8d66d0b92ec211594ee6

Hypothesis: in a seeded synthetic model of 10000 landed liquidations (health factor uniform on 0.95 to 1.0; oracle
update landing one block after the local state read in 40% of rows, shift absolute value of a Gaussian with standard
deviation 0.012; omitted earlier same-block transaction in 30% of rows, standard deviation 0.005; omitted balance change
in 10% of rows, standard deviation 0.008), the count of flips attributable to the oracle-timing input alone exceeds the
count attributable to the earlier-transaction input alone, and with no input omitted the flip count is exactly zero.

Metric: /stalestate3/oracle_timing_only_count. Expected: greater than /stalestate3/tx_only_count, with the no-omission
flip count equal to 0 (range 100 to 10000).
Falsifier: oracle_timing_only_count not greater than tx_only_count, or a nonzero flip count with no omitted input.
Limit: every parameter is an assumption, not a measured value; no chain data is used. Shifts are applied upward to the
stale-state health factor; a flip is a stale health factor at or above 1. A row counts toward an input's alone count
when only that input's shift alone flips the flag; rows needing two or more inputs go to a multi-input count.

## 2026-11-13T13:50:02Z stalestate4 proposal:4c562e8eabdc8dc2ad736c74695f6f960841afdfbd68cfbdb22576f0ad3fdf1b

Hypothesis: in a seeded synthetic model of 10000 landed liquidations with two in-transaction price reads (health factor
uniform on 0.95 to 1.0; the landed path splits the two reads across an oracle update in 25% of rows, shift absolute
value of a Gaussian with standard deviation 0.01; the local simulation resolves both reads to one round), the count of
flag flips attributable to the multi-read input alone is greater than 0, and with no split the multi-read flip count is
exactly 0.

Metric: /stalestate4/multiread_divergence_count. Expected: greater than 0, with the no-split control count equal to 0
(range 100 to 10000).
Falsifier: multiread_divergence_count equal to 0, or a nonzero flip count in the no-split control arm.
Limit: every parameter is an assumption, not a measured value; no chain data is used. A flip is a stale health factor at
or above 1 after the shift; rows whose health factor is already at or above 1 are counted as unexplained residual.

## 2026-11-13T13:53:30Z stalestate5 proposal:e32a4ac36cf744911db6fd2e9a18d197c01fb54e64265660648c9b558f07288a

Hypothesis: in a seeded synthetic model of 10000 landed liquidations (health factor uniform on 0.95 to 1.0; the local
simulation reads a block-context field that differs from the landed block in 20% of rows, shift absolute value of a
Gaussian with standard deviation 0.01), the count of flag flips attributable to the environment input alone is greater
than 0, and with identical environment fields the flip count is exactly 0.

Metric: /stalestate5/environment_divergence_count. Expected: greater than 0, with the identical-environment control
count equal to 0 (range 100 to 10000).
Falsifier: environment_divergence_count equal to 0, or a nonzero flip count in the identical-environment control arm.
Limit: every parameter is an assumption, not a measured value; no chain data is used. A flip is a stale health factor at
or above 1 after the shift; rows whose health factor is already at or above 1 are counted as unexplained residual.

## 2026-11-13T14:09:02Z reviewerdet proposal:6813aefee8e6f3b01a04899e948d9a0a8f71c74fc0e161d2daa3e2b22dd2f000

Hypothesis: on 1000 seeded synthetic flag cases reviewed 5 times by a scripted reviewer with a fixed per-case systematic
error (same wrong label every run) on 10 percent of cases and no random noise, the run-to-run disagreement count is 0
while the count of cases wrong against planted truth is at least 80; adding 5 percent random per-run noise makes the
disagreement count exceed 0.

Metric: /reviewer/run_disagreement_count. Expected: equal to 0 for the systematic-error reviewer, with
wrong_vs_truth_count at least 80.
Falsifier: a nonzero run_disagreement_count for the noise-free reviewer, or fewer than 80 cases wrong against planted truth.
Limit: every parameter is an assumption; all inputs are synthetic and seeded; no chain data is used.

## 2026-11-16T11:14:00Z denimwin proposal:19e67f19bfabaf1a250bd2f55ad273aa69563aac0f1163b9f14f6016d77111fd

Hypothesis: on Base mainnet, the window of 600 blocks starting at block 53556127 spans less than 0.15 of the header
timestamp wall time of the window of 600 blocks ending at block 53556126, because the post-activation block interval is
much shorter than the pre-activation one. A ratio of 0.5 or more would mean the documented shorter block interval does
not show in block headers.

Metric: /windows/0/wall_time_ratio. Expected: at most 0.15 (range 0.05 to 0.15), with the per-block median interval
reported alongside.
Falsifier: a ratio at or above 0.5, or a header read that cannot pin the boundary from chain data (the run then reports
abstain instead of a number).
Limit: header timestamps are whole seconds, so the ratio is coarse; read-only header reads from one node; no
transactions, no traces.

## 2026-11-16T11:20:00Z denimorder proposal:695905a809383c181c2eff54d4d318f773c046aa43e233ea71e13777646c896a

Hypothesis: on Base mainnet, in the 200 blocks starting at block 53556127, at least 0.90 of adjacent non-system
transaction pairs within a block have a non-increasing effective priority fee, and that share is at least 0.10 higher
than in the 200 blocks ending at block 53556126. A post share below 0.70, or a difference under 0.05, means landed
blocks do not show the documented ordering.

Metric: /ranges/0/ordered_pair_share. Expected: at least 0.90 (range 0.90 to 1.0).
Falsifier: post share below 0.70, or less than 0.05 above the pre-activation control range; fewer than 1000 adjacent
pairs in a range makes the run report abstain, not a number.
Limit: effective priority fee is computed from the transaction fields and the block base fee; deposit-type (0x7e)
transactions are excluded; read-only block reads from one node; no traces, no receipts.

## 2026-11-16T11:34:44Z denimdrift proposal:bc6e7f2772ab7e33ae9e81da3fd9d745829d98778721618ff621e204e492ac63

Hypothesis: on Base mainnet, in the 150 blocks starting at block 53556127, for landed non-system transactions with a
nonempty call target, the share whose eth_call success flag at the parent block differs from the receipt status is at
least 0.05 higher when an earlier transaction in the same block targeted the same address (touched) than when none did
(untouched). A difference under 0.01 means same-block contention on one address does not explain the parent-state
replay gap.

Metric: /ranges/0/divergent_share_touched. Expected: at least 0.08 (range 0.08 to 0.80), at least 0.05 above the
untouched share.
Falsifier: a difference under 0.01, fewer than 300 replayed touched transactions (the run then reports abstain), or a
node that does not serve state at the parent block.
Limit: eth_call uses the sender, target, value, input and gas of the landed transaction at the parent block state, with
block context of the parent; read-only; samples capped at 400 per group, seeded; no traces.

## 2026-11-16T11:40:55Z denimempty proposal:7e2dd5bb6a349d584b45b167d32d91af62a433644ed0b0775606e716aa9ba57e

Hypothesis: on Base mainnet, the share of blocks containing no non-system (non-0x7e) transactions in the 600 blocks
starting at block 53556127 exceeds the share in the 600 blocks ending at block 53556126 by at least 0.05 in absolute
terms.

Metric: /ranges/0/empty_block_share. Expected: post share at least 0.05 above the pre control (range 0.05 to 1.0).
Falsifier: post share minus pre share below 0.05 on the pinned ranges.
Limit: blocks with only deposit-type (0x7e) transactions count as empty; read-only block reads from one node; no
receipts, no traces.

## 2026-11-16T11:46:33Z denimfee proposal:e6c4a277b1d54a15c870c520b8f3e39cba1fe5c88df1e6f49c64f89ed1f5fe28

Hypothesis: on Base mainnet, the median per-block gasLimit over the 600 blocks starting at block 53556127 is at most
0.2 times the median per-block gasLimit over the 600 blocks ending at block 53556126.

Metric: /comparison/gas_limit_median_ratio. Expected: post/pre ratio at most 0.2 (range 0.0 to 0.2).
Falsifier: post median gasLimit divided by pre median gasLimit above 0.2 on the pinned ranges.
Limit: header reads from one node; also reports gas-used share of the limit and the share of blocks whose base fee
rose versus the previous block; no receipts, no traces.

## 2026-11-16T12:01:24Z denimoracle proposal:3746f40d2fef700ab8024f6d3f416716864ffdda4577ec29d1b5e43c6cdb0b28

Hypothesis: on the 150000 pinned Base blocks starting at block 53556127, at least 0.30 of blocks containing an Aave V3
pool LiquidationCall log also contain a Chainlink AnswerUpdated log (any feed address), provided at least 20 liquidation
blocks are found.

Metric: /ranges/0/same_block_update_share. Expected: at least 0.30 (range 0.30 to 1.0).
Falsifier: share below 0.30 with at least 20 liquidation blocks; with fewer the run reports abstain.
Limit: only AnswerUpdated logs count as feed updates (OCR2 NewTransmission-only feeds and SVR auction updates that do
not emit it are missed, so the share is a lower bound); also reports the median transaction-index offset (liquidation
minus first update in the block); aggregates only; read-only getLogs, no traces.

## 2026-11-16T13:42:21Z denimoraclepre proposal:13c181c337c039c8354cbac5fe903e94811d68a3aade5c0f26a3e479532c79e4

Hypothesis: on the 600000 pinned Base blocks 52956127 to 53556126 (pre-Denim, 2s blocks), at least 0.30 of blocks
containing an Aave V3 pool LiquidationCall log also contain a Chainlink AnswerUpdated log, provided at least 20
liquidation blocks are found.

Metric: /ranges/0/same_block_update_share. Expected: at least 0.30 (range 0.30 to 1.0).
Falsifier: share below 0.30 with at least 20 liquidation blocks; with fewer the run reports abstain.
Limit: same as denimoracle (AnswerUpdated only, so the share is a lower bound); no code change; read-only getLogs.

## 2026-11-16T13:57:55Z denimemptypersist proposal:2568d046599015d5564ed5e14da34166da795e4374e319723a8d288e3731f72f

Hypothesis: on Base mainnet, the share of blocks holding no non-0x7e transactions in the 2000 blocks starting at block
55556127 exceeds the share in the 2000 blocks ending at block 53556126 by at least 0.05 in absolute terms.

Metric: /ranges/0/empty_block_share (post window). Expected: at least 0.05 (range 0.05 to 1.0), pre control near 0.
Falsifier: post share minus pre share below 0.05 on the pinned ranges.
Limit: denimempty's adjacency check is relaxed to post_start > pre_end; the provenance command now records the real
out path; one node, full-block reads, no receipts; abstain (error) if block 55556127 does not exist on the node.

## 2026-11-17T14:48:48Z mainoracleimmediate proposal:d4f69826f2a018d9c13fda0c6c94eed16d50bb01786f0cfc6f1f1faa4e574450

Hypothesis: on Ethereum mainnet blocks 23000000 to 23099999, among Aave V3 pool LiquidationCall transactions in blocks
that also contain a Chainlink AnswerUpdated log, at least 0.40 sit at transaction index (some AnswerUpdated transaction
index + 1), provided at least 20 such liquidation transactions are found.

Metric: /ranges/0/immediate_follow_share. Expected: at least 0.40 (range 0.40 to 1.0).
Falsifier: share below 0.40 with at least 20 same-block liquidation transactions; with fewer the run reports abstain.
Limit: only AnswerUpdated logs count as feed updates (lower bound on updates); aggregates only; read-only getLogs, no
traces. The endpoint is resolved from the platform API at run time and never printed.

## 2026-11-17T14:53:41Z mainoracleimmediate2 proposal:eaaf4c66dbfdcc9bf91aa64ab83eb09e0c63655489c18be8f019eaee85f79a17

Hypothesis: on Ethereum mainnet blocks 23100000 to 23199999, among Aave V3 LiquidationCall transactions in blocks that
also contain a Chainlink AnswerUpdated log, at least 0.30 sit at transaction index (some AnswerUpdated transaction
index + 1), provided at least 20 such transactions are found.

Metric: /ranges/0/immediate_follow_share. Expected: at least 0.30 (range 0.30 to 1.0).
Falsifier: share below 0.30 with at least 20 same-block liquidation transactions; with fewer the run reports abstain.
Limit: no code change, replication on a disjoint range; AnswerUpdated only; read-only getLogs.

## 2026-11-17T14:58:32Z mainoracleimmediatesplit proposal:389e2e481f8c2801bf0bdb39f9edb373166dfa5dd0653bc066b7900c3e056d1f

Hypothesis: on Ethereum mainnet blocks 23200000 to 23299999, among Aave V3 LiquidationCall transactions that carry exactly
one LiquidationCall log and sit in blocks that also contain a Chainlink AnswerUpdated log, at least 0.30 sit at
transaction index (some AnswerUpdated transaction index + 1), provided at least 20 such transactions are found.

Metric: /ranges/0/buckets/0/immediate_follow_share (single-log bucket). Expected: at least 0.30 (range 0.30 to 1.0).
Falsifier: single-log bucket share below 0.30 with at least 20 same-block transactions in that bucket; with fewer the run
reports abstain. Bucket 1 holds transactions with two or more logs; log count is a labelled size proxy only.
Limit: AnswerUpdated only; read-only getLogs; Wilson 95% intervals reported.

## 2026-11-17T15:03:41Z mainoracleimmediatesize proposal:f3b417d32385870c75a993c0e790a42d488fa83d3e5b8c27b0afc831b7481fd1

Hypothesis: on Ethereum mainnet blocks 23300000 to 23499999, among Aave V3 LiquidationCall transactions with a single log
whose debt asset is USDC or USDT and that sit in blocks also containing a Chainlink AnswerUpdated log, the share at
transaction index (some AnswerUpdated transaction index + 1) is at least 0.30 in the bucket with debtToCover of 1000 units
or more, provided at least 20 such transactions are found in that bucket.

Metric: /ranges/0/buckets/1/immediate_follow_share. Expected: at least 0.30 (range 0.30 to 1.0).
Falsifier: large-bucket share below 0.30 with at least 20 same-block transactions in that bucket; with fewer the run
reports abstain. Bucket 0 holds debtToCover below 1000 units.
Limit: AnswerUpdated only; read-only getLogs; Wilson 95% intervals reported; USDC and USDT only (6 decimals).

## 2026-11-17T15:08:34Z mainoracleimmediatepay proposal:92b79188fb39ac5d1838009e973dd3b01bed05d45f8510b48f555f13b64892e0

Hypothesis: on Ethereum mainnet blocks 23500000 to 23599999, among Aave V3 LiquidationCall transactions at transaction
index (some AnswerUpdated transaction index + 1) in blocks that also contain a Chainlink AnswerUpdated log, at least 0.40
have an exactly zero priority fee (effectiveGasPrice equal to the block base fee), provided at least 20 such transactions
are found.

Metric: /ranges/0/immediate_follow_zero_priority_share. Expected: at least 0.40 (range 0.40 to 1.0).
Falsifier: share below 0.40 with at least 20 immediate-follow transactions; with fewer the run reports abstain.
Limit: AnswerUpdated only; read-only getLogs and receipts; Wilson 95% intervals; a zero priority fee does not show
whether a coinbase transfer was paid.

## 2026-11-17T15:18:22Z mainoracleimmediatecb proposal:b871762a2637200753c3b0944ad3afbdab1a5cb8a3ca123ef747f2367921065b

Hypothesis: on Ethereum mainnet blocks 23600000 to 23699999, among Aave V3 LiquidationCall transactions at transaction
index (some AnswerUpdated transaction index + 1) in blocks that also contain a Chainlink AnswerUpdated log, at least 0.30
contain a call with nonzero ETH value sent to the block header coinbase (found by trace_transaction), provided at least 20
such transactions are found.

Metric: /ranges/0/immediate_follow_coinbase_transfer_share. Expected: at least 0.30 (range 0.30 to 1.0).
Falsifier: share below 0.30 with at least 20 immediate-follow transactions; with fewer the run reports abstain.
Limit: AnswerUpdated only; read-only getLogs, headers and trace_transaction; Wilson 95% intervals; split by zero versus
nonzero priority fee.

## 2026-11-17T15:23:23Z mainoracleimmediatecbsplit proposal:c6f028dd6e7b2fb7cbd576acc479a14f4d923ab76b40cb35dbff76b6c28b3029

Hypothesis: on Ethereum mainnet blocks 23700000 to 23899999, the share of Aave V3 LiquidationCall transactions containing
a call with nonzero ETH value sent to the block header coinbase (found by trace_transaction) is at least 0.20 higher among
immediate-follow transactions (index equal to some AnswerUpdated index + 1 in the same block) than among all other
liquidation transactions, provided each arm holds at least 20 transactions.

Metric: /ranges/0/share_difference_immediate_minus_other. Expected: at least 0.20 (range 0.20 to 1.0).
Falsifier: difference below 0.20 with at least 20 transactions in each arm; with fewer in either arm the run reports abstain.
Limit: AnswerUpdated only; read-only getLogs, headers and trace_transaction; Wilson 95% intervals per arm.

## 2026-11-17T15:40:15Z mainoracleupdatepair proposal:5ed5b836190f60f4333a2fa24a732d0d4ac0abab4959c9104716d5a80b869539

Hypothesis: on Ethereum mainnet blocks 23900000 to 24099999, among Chainlink AnswerUpdated transactions (distinct block and
transaction index) in blocks that hold an Aave V3 LiquidationCall, the share whose next transaction index holds a
LiquidationCall is at most 0.25, provided at least 20 update transactions exist.

Metric: /ranges/0/update_followed_by_liquidation_share. Expected: at most 0.25 (range 0.0 to 0.25).
Falsifier: share above 0.25 with at least 20 update transactions; with fewer the run reports abstain.
Limit: AnswerUpdated only; read-only getLogs; Wilson 95% interval reported.

## 2026-11-17T15:49:13Z mainoracleimmediatecbshare proposal:d9b5e7e0b3ecbff285451f00ada04d569e494504f055a2e514beed366d0077c7

Hypothesis: on Ethereum mainnet blocks 24100000 to 24299999, among Aave V3 LiquidationCall transactions with positive total
builder payment (priority fee times gas used plus traced nonzero ETH value sent to the header coinbase), the median
coinbase-transfer share of that payment is at least 0.50, provided at least 20 such transactions exist.

Metric: /ranges/0/coinbase_share_of_builder_payment_median. Expected: at least 0.50 (range 0.50 to 1.0).
Falsifier: median share below 0.50 with at least 20 paying transactions; with fewer the run reports abstain.
Limit: priority fee is (effectiveGasPrice minus block base fee) times gasUsed; trace calls carrying an error are not
counted; zero-payment transactions are counted separately; read-only getLogs, headers, receipts and trace_transaction.

## 2026-11-17T15:56:48Z mainoraclesendershare proposal:8d6e6dcce2606268ec9bf02931a99dc1fd6d9b22c3fe75e6d9b23f780f18fc16

Hypothesis: on Ethereum mainnet blocks 24300000 to 24499999, among Aave V3 LiquidationCall transactions (at least 50
required, else abstain), the top-3 tx.from senders together account for less than 0.50 of liquidation transactions.

Metric: /ranges/0/top3_sender_share. Expected: below 0.50 (range 0.20 to 0.50).
Falsifier: top3_sender_share of at least 0.50 with at least 50 liquidation transactions in the range.
Limit: ranks only, no addresses written; read-only getLogs and transactions.

## 2026-11-17T16:04:35Z mainoraclesendercb proposal:9b983d3ae8f6874533d2e71759e2e30043f329f24c100767576c45abd9e89a37

Hypothesis: on Ethereum mainnet blocks 24500000 to 24599999, among Aave V3 LiquidationCall transactions (at least 50 from
top-3 senders and at least 50 from other senders, else abstain; if the trace budget binds, trace the first 600 of each
group in block and index order), the share with a nonzero traced coinbase transfer to the header miner is at least 0.10
higher for the top-3 senders than for the other senders.

Metric: /ranges/0/top3_minus_rest_cb_share. Expected: at least 0.10 (range 0.10 to 0.60).
Falsifier: top3_minus_rest_cb_share below 0.10 with both groups at or above 50 transactions.
Limit: senders ranked by tx.from count, no addresses written; read-only getLogs, headers, trace_transaction.

## 2026-11-17T16:11:42Z mainoraclesizecb proposal:a26b0710047112682cacbef3d895c3fae41690889744b195b7e7b0e1219690be

Hypothesis: on Ethereum mainnet blocks 24600000 to 24799999, among single-log Aave V3 liquidation transactions with USDC or
USDT debt (at least 30 in each bucket, else abstain; if the trace budget binds, trace the first 600 per bucket in block and
index order), the share with a nonzero traced coinbase transfer to the header miner is at least 0.10 higher in the
at-least-1000-units debtToCover bucket than in the below-1000-units bucket.

Metric: /ranges/0/large_minus_small_cb_share. Expected: at least 0.10 (range 0.10 to 0.80).
Falsifier: large_minus_small_cb_share below 0.10 with both buckets at or above 30 transactions.
Limit: read-only getLogs, headers, trace_transaction; no addresses written.

## 2026-11-17T16:18:00Z mainoraclesametx proposal:0fa5cce702c542bafa9c4c8cd7315ae1f38a76ca6f951433d2e544b4542bb554

Hypothesis: on Ethereum mainnet blocks 24800000 to 24999999, among Aave V3 LiquidationCall transactions in blocks that also
contain a Chainlink AnswerUpdated log (at least 50 required, else abstain), at most 0.10 are in a transaction that itself
carries an AnswerUpdated log.

Metric: /ranges/0/same_tx_as_oracle_update_share. Expected: at most 0.10 (range 0.0 to 0.10).
Falsifier: same_tx_as_oracle_update_share above 0.10 with at least 50 same-block liquidation transactions.
Limit: read-only getLogs; no traces, no addresses written.

## 2026-11-17T16:23:17Z mainoraclereliq proposal:c3f3efce95f384c89f294a0e13ce963a82052e443dbc3a602a822f739a5e031d

Hypothesis: on Ethereum mainnet blocks 25000000 to 25199999, among Aave V3 LiquidationCall logs (at least 100 required,
else abstain), at least 0.10 target a borrower that already had a LiquidationCall log in the previous 100 blocks.

Metric: /ranges/0/reliq_share. Expected: at least 0.10 (range 0.10 to 0.60).
Falsifier: reliq_share below 0.10 with at least 100 liquidation logs in the range.
Limit: read-only getLogs; no traces, no addresses written. A different measure from a refund-transfer count.

## 2026-11-17T16:31:12Z replayshift proposal:21103710a1638e5b1694ddf109f197f607b89dd1082517fc47ace51348845e18

Hypothesis: on Ethereum mainnet blocks 25200000 to 25399999, among landed Aave V3 LiquidationCall transactions (at least 100
required, else abstain), at least 5 have an eth_call replay (same from, to, input, value) whose success flag at the parent
block state differs from its success flag at the state of the block before the parent.

Metric: /ranges/0/state_shift_differs_count. Expected: at least 5 (range 5 to 200).
Falsifier: state_shift_differs_count below 5 with at least 100 replayed liquidation transactions.
Limit: read-only getLogs, transactions, eth_call at fixed historical blocks; no addresses written.

## 2026-11-17T16:40:00Z mainoraclespec proposal:98c0bd49a0e0c2192e01e341aa11ff66b787b7601f7074970cad528c5fa88726

Hypothesis: on Ethereum mainnet blocks 25400000 to 25599999, among Aave V3 LiquidationCall transactions (at least 100
required, else abstain), at least 0.10 have no AnswerUpdated log in an earlier transaction of the same block or in the 2
preceding blocks (the speculative class).

Metric: /ranges/0/speculative_share. Expected: at least 0.10 (range 0.10 to 0.60).
Falsifier: speculative_share below 0.10 with at least 100 liquidation transactions in the range.
Limit: read-only getLogs; no traces, no addresses written. A liquidation in the same transaction as an update is not an
earlier-transaction update and is counted separately.

## 2026-11-17T16:51:48Z buildercb2 proposal:3d9ddb1ab8d71f4d18de6b8e545d50d7869d887658a073e85812558e5202288b

Hypothesis: on Ethereum mainnet blocks 22600000 to 22799999, among traced Aave V3 liquidation transactions (at least 100
required, else the metric is written as null with abstain true), the coinbase-transfer share (a traced call with nonzero
value to the header miner) of transactions landed by the most frequent builder in the range is at least 0.10 higher than
the share among transactions landed by all other builders.

Metric: /ranges/0/top1_minus_rest_cb_share. Expected: at least 0.10 (range 0.10 to 0.80).
Falsifier: top1_minus_rest_cb_share below 0.10 with at least 100 traced liquidation transactions in the range.
Limit: read-only getLogs, headers, trace_transaction; builders ranked by frequency only, no address written. Retry of the
236 design on a different range.

## 2026-11-17T17:05:30Z speccb2 proposal:6a04c3dcbbf42e3204207727e7576b75708e9952577ed0262aa8746150bb56ad

Hypothesis: on Ethereum mainnet blocks 22800000 to 23199999, among traced Aave V3 liquidation transactions (at least 100
required, each class at least 20, else the metric is written as null with abstain true), the coinbase-transfer share (a
traced call with nonzero value to the header miner) of speculative transactions (no AnswerUpdated log in an earlier
transaction of the same block or the 2 preceding blocks) is at least 0.10 higher than that of non-speculative ones.

Metric: /ranges/0/spec_minus_nonspec_cb_share. Expected: at least 0.10 (range 0.10 to 0.80).
Falsifier: spec_minus_nonspec_cb_share below 0.10 with at least 100 traced liquidation transactions and each class at least 20.
Limit: read-only getLogs, headers, trace_transaction; no addresses written. Retry of the 238 design on a different range.

## 2026-11-17T17:14:18Z specsize proposal:67769b2f514190f0354e67690868d63d209b420f1bb7143188805095156f0d40

Hypothesis: on Ethereum mainnet blocks 22800000 to 23199999, among single-log Aave V3 USDC/USDT-debt liquidation
transactions (at least 100 required, each bucket at least 20, else the metric is written as null with abstain true), the
speculative share (no AnswerUpdated log in an earlier transaction of the same block or the 2 preceding blocks) of the
debtToCover at-least-1000-unit bucket is at least 0.10 lower than that of the below-1000-unit bucket.

Metric: /ranges/0/large_minus_small_spec_share. Expected: at most -0.10 (range -1.0 to -0.10).
Falsifier: large_minus_small_spec_share above -0.10 with at least 100 transactions and each bucket at least 20.
Limit: read-only getLogs; no traces, no addresses written.

## 2026-11-17T17:22:15Z specsize3 proposal:d4eed528f998773ba64959e67cffd00bfd64b8870735e8a93dbac1241f812124

Hypothesis: on Ethereum mainnet blocks 23200000 to 23599999, among single-log Aave V3 USDC/USDT-debt liquidation
transactions (at least 100 required, each bucket at least 20, else the metric is written as null with abstain true), the
speculative share of the debtToCover at-least-1000-unit bucket is lower than that of the below-1000-unit bucket by at least 0.05.

Metric: /ranges/0/large_minus_small_spec_share. Expected: at most -0.05 (range -1.0 to -0.05).
Falsifier: large_minus_small_spec_share above -0.05 with at least 100 transactions and each bucket at least 20.
Limit: read-only getLogs; no traces, no addresses written. Unchanged rerun of the specsize subcommand on a disjoint range.

## 2026-11-17T17:29:29Z specthresh proposal:21c9cd0fe93847dc45fb59d72fb36583c2fed29f6377e1f1db0d00bcbac9ae30

Hypothesis: on Ethereum mainnet blocks 23600000 to 23999999, among single-log Aave V3 USDC/USDT-debt liquidation
transactions (at least 100 total and at least 20 per bucket at every cutoff, else the metric is null with abstain true),
the large-minus-small speculative share is negative at all three debtToCover cutoffs (250, 1000, 4000 units).

Metric: /ranges/0/negative_cutoff_count. Expected: equals 3 (range 3 to 3).
Falsifier: negative_cutoff_count below 3 with at least 100 transactions and at least 20 per bucket at every cutoff.
Limit: read-only getLogs; no traces, no addresses written. Reuses the specsize classification with the cutoff varied.

## 2026-11-17T17:35:22Z spectail proposal:4de90ac4bf00e4c4ddf4d1969efb5a26db9d229bd7046fa3105b2b7a3763231f

Hypothesis: on Ethereum mainnet blocks 23200000 to 23999999, among single-log Aave V3 USDC/USDT-debt liquidation
transactions (at least 100 total and at least 20 in the tail, else the metric is null with abstain true), the speculative
share of the at-least-20000-unit debtToCover tail is lower than that of the below-20000-unit rest by at least 0.02.

Metric: /ranges/0/tail_minus_rest_spec_share. Expected: at most -0.02 (range -1.0 to -0.02).
Falsifier: tail_minus_rest_spec_share above -0.02 with at least 100 transactions and at least 20 in the tail.
Limit: read-only getLogs; no traces, no addresses written. Reuses the specsize classification with a 20000-unit cutoff.

## 2026-11-17T17:44:56Z spectail3 proposal:9216651a844ac9e535f3a1e020043466287c3e0d747f008f730f7779a3fb7255

Hypothesis: on Ethereum mainnet blocks 22800000 to 23199999, among single-log Aave V3 USDC/USDT-debt liquidation
transactions (at least 100 total and at least 20 in the tail, else the metric is null with abstain true), the speculative
share of the at-least-20000-unit debtToCover tail is lower than that of the below-20000-unit rest by at least 0.02
(a replication of the 245 tail result on a disjoint range).

Metric: /ranges/0/tail_minus_rest_spec_share. Expected: at most -0.02 (range -1.0 to -0.02).
Falsifier: tail_minus_rest_spec_share above -0.02 with at least 100 transactions and at least 20 in the tail.
Limit: read-only getLogs; no traces, no addresses written. Unchanged rerun of the mainoraclespectail subcommand.

## 2026-11-17T17:49:25Z specsweep proposal:48821ce4fb229e7c91694f1c020c3db0e2c90996e7b029c1d3d4becb9f85be9f

Hypothesis: on Ethereum mainnet blocks 22800000 to 23999999, among single-log Aave V3 USDC/USDT-debt liquidation
transactions (at least 100 total, and at least 20 in a cutoff's tail, else that cutoff abstains and the metric is null
if all abstain), the tail speculative share stays at or below 0.05 at every non-abstaining cutoff (10000, 20000, 40000
units), so the zero-tail result from 245 and 247 is not an artefact of the 20000-unit cutoff.

Metric: /ranges/0/max_tail_spec_share. Expected: at most 0.05 (range 0.0 to 0.05).
Falsifier: max_tail_spec_share above 0.05 with at least one non-abstaining cutoff having at least 20 tail txs.
Limit: read-only getLogs; no traces, no addresses written. Reuses the spectail classification with the cutoff varied.

## 2026-11-17T18:02:50Z specsweep3 proposal:cf72b65cc03a52815fee3069f656c8670445403251d3d949b6de77ca7110640a

Hypothesis: on Ethereum mainnet blocks 24000000 to 25199999, among single-log Aave V3 USDC/USDT-debt liquidation
transactions (at least 100 total, and at least 20 in a cutoff's tail, else that cutoff abstains and the metric is null
if all abstain), the tail speculative share stays at or below 0.05 at every non-abstaining cutoff (10000, 20000, 40000
units), extending the zero-tail result from 245, 247 and 248 to a third disjoint range.

Metric: /ranges/0/max_tail_spec_share. Expected: at most 0.05 (range 0.0 to 0.05).
Falsifier: max_tail_spec_share above 0.05 with at least one non-abstaining cutoff having at least 20 tail txs.
Limit: read-only getLogs; no traces, no addresses written. Unchanged rerun of the mainoraclespecsweep subcommand.

## 2026-11-17T18:19:30Z specsweep4 proposal:e47bbeb818c5172a302307e7398c57d7270263aafdc8c0e5081fa89f926b8dae

Hypothesis: on Ethereum mainnet blocks 21600000 to 22799999 the tail_spec_share at threshold_units 10000 is nonzero,
as on 24000000 to 25199999 and unlike the 22800000 ranges.

Metric: /ranges/0/cutoffs/0/tail_spec_share. Expected: greater than 0.0 and at most 0.05 (range 0.0001 to 0.05).
Falsifier: tail_spec_share at threshold_units 10000 equals 0.0, or the run abstains for lack of tail txs.
Limit: read-only getLogs; no traces, no addresses written. Unchanged rerun of the mainoraclespecsweep subcommand.

## 2026-11-17T18:29:30Z specsweep5 proposal:7fd62b62d8663ab92f9e7adfb9b4238595450b27b91c5de683b438a3d41cb192

Hypothesis: on Ethereum mainnet blocks 20400000 to 21599999, among single-log Aave V3 USDC/USDT-debt liquidation
transactions (at least 100 total, and at least 20 in a cutoff's tail, else that cutoff abstains and the metric is null
if all abstain), the tail speculative share stays at or below 0.05 at every non-abstaining cutoff (10000, 20000, 40000
units), on a fifth disjoint range.

Metric: /ranges/0/max_tail_spec_share. Expected: at most 0.05 (range 0.0 to 0.05).
Falsifier: max_tail_spec_share above 0.05 with at least one non-abstaining cutoff having at least 20 tail txs, or all cutoffs abstaining so the metric is null.
Limit: read-only getLogs; no traces, no addresses written. Unchanged rerun of the mainoraclespecsweep subcommand.

## 2026-11-17T18:38:00Z specsweep6 proposal:fa0312bbb075aef91fae6d22ae756b66eafa2caa36c977d866008966fa569af2

Hypothesis: on Ethereum mainnet blocks 19200000 to 20399999, among single-log Aave V3 USDC/USDT-debt liquidation
transactions (at least 100 total, and at least 20 in a cutoff's tail, else that cutoff abstains and the metric is null
if all abstain), the tail speculative share stays at or below 0.05 at every non-abstaining cutoff (10000, 20000, 40000
units), on a sixth disjoint range.

Metric: /ranges/0/max_tail_spec_share. Expected: at most 0.05 (range 0.0 to 0.05).
Falsifier: max_tail_spec_share above 0.05 with at least one non-abstaining cutoff having at least 20 tail txs, or all cutoffs abstaining so the metric is null.
Limit: read-only getLogs; no traces, no addresses written. Unchanged rerun of the mainoraclespecsweep subcommand.

## 2026-11-17T18:44:40Z specsweep7 proposal:02b394108bf2e9b5773b5ccc8ee71bd1fb6006ad835794251dd2cc9819c51319

Hypothesis: on Ethereum mainnet blocks 18000000 to 19199999, among single-log Aave V3 USDC/USDT-debt liquidation
transactions (at least 100 total, and at least 20 in a cutoff's tail, else that cutoff abstains and the metric is null
if all abstain), the tail speculative share stays at or below 0.05 at every non-abstaining cutoff (10000, 20000, 40000
units), on a sixth disjoint range.

Metric: /ranges/0/max_tail_spec_share. Expected: at most 0.05 (range 0.0 to 0.05).
Falsifier: max_tail_spec_share above 0.05 with at least one non-abstaining cutoff having at least 20 tail txs, or all cutoffs abstaining so the metric is null.
Limit: read-only getLogs; no traces, no addresses written. Unchanged rerun of the mainoraclespecsweep subcommand.

## 2026-11-17T18:49:30Z specsweep8 proposal:30e0845d18021aaf607695bee0be6085d6c3defc438e72c5298f084013ab9f32

Hypothesis: on Ethereum mainnet blocks 16800000 to 17999999, among single-log Aave V3 USDC/USDT-debt liquidation
transactions (at least 100 total, and at least 20 in a cutoff's tail, else that cutoff abstains and the metric is null
if all abstain), the tail speculative share stays at or below 0.05 at every non-abstaining cutoff (10000, 20000, 40000
units), on a seventh disjoint range.

Metric: /ranges/0/max_tail_spec_share. Expected: at most 0.05 (range 0.0 to 0.05).
Falsifier: max_tail_spec_share above 0.05 with at least one non-abstaining cutoff having at least 20 tail txs, or all cutoffs abstaining so the metric is null.
Limit: read-only getLogs; no traces, no addresses written. Unchanged rerun of the mainoraclespecsweep subcommand.

## 2026-11-19T19:20:02Z tracereplay proposal:800ddc843394c95b216c23043c1a3f457e7190e98beca397e5a319826a019d92

Hypothesis: replaying landed Aave V3 liquidation transactions on parent-block state without the earlier same-block
transactions diverges from the receipt (status or LiquidationCall log count) for at least 10% of the first 50
liquidations in blocks 23543616..23544615.

Metric: /exact_fail_count (replays not matching the receipt). Expected: at least 5 of 50 (range 5 to 50).
/exact_fail_count and /replayed_n are always written, even if replayed_n is below 50.
Falsifier: exact_fail_count below 5 with replayed_n of 50.
Limit: read-only (eth_getLogs, eth_getTransactionReceipt, eth_getTransactionByHash, debug_traceCall with the callTracer
at the parent block); no addresses written. Same-block predecessors are deliberately not applied, so divergence measures
that omission, not a replay bug.

## 2026-11-19T19:30:59Z tracereplay-stress proposal:6ebd7e9fb684fe359f9d215c2a817fe9e24a843a859f5737faad65b5baf1ea6d

Hypothesis: re-executing landed Aave V3 LiquidationCall transactions with debug_traceCall at the parent block diverges
from the receipt (status or summed LiquidationCall log count) for at least 5 of up to 50 replayed transactions in
blocks 23549000 to 23549999, because same-block predecessors are not applied.

Metric: /exact_fail_count. Expected: increases from 0 to at least 5 (range 5 to 50), with replayed_n above 0.
Falsifier: replayed_n of at least 20 with exact_fail_count below 5 refutes it; replayed_n of 0 is a sizing null that does not test it.
Limit: read-only; no addresses written. Unchanged rerun of the tracereplay subcommand.

## 2026-11-19T19:35:02Z tracereplay-predecessors proposal:279f6d8efd1583173170bc0cf6ee47be71f7bff945d79324a4d01c6afc51e0d7

Hypothesis: with same-block predecessors applied (debug_traceTransaction), at most 10 of the same 50 Aave V3
liquidation transactions in blocks 23549000 to 23549999 diverge from the receipt (status or summed LiquidationCall
log count), versus 43 with debug_traceCall at the parent block.

Metric: /exact_fail_count_trace_transaction. Expected: at most 10 of 50 (range 0 to 10).
Falsifier: exact_fail_count_trace_transaction above 10 with replayed_n of 50; a trace error on most transactions is a null that does not test it.
Limit: read-only (debug_traceTransaction with the callTracer); no addresses written. New --mode trace-transaction on tracereplay.

## 2026-11-19T19:43:01Z 282-tracecall-window2 proposal:7abc94e7430ac9ce656fb262c5c0060483d5a53ab39d534394c7f8b822ac8af7

Hypothesis: replaying landed Aave V3 LiquidationCall transactions with debug_traceCall at the parent block, without
same-block predecessors, diverges from the receipt (status or summed LiquidationCall log count) for at least 50% of up
to 100 transactions in blocks 23550000 to 23550999, as it did for 43 of 50 in blocks 23549000 to 23549999.

Metric: /exact_fail_count. Expected: at least 50 of up to 100 replayed (mismatch share at least 0.50), with replayed_n above 0.
Falsifier: replayed_n of at least 50 with exact_fail_count below half of replayed_n refutes it; replayed_n of 0 is a sizing null.
Limit: read-only; no addresses written. Unchanged rerun of tracereplay in trace-call mode.

## 2026-11-19T19:49:12Z 284-tracetx-window2 proposal:68484ab3f7f3eacf5a5a60d00871b1d9e4c0ceddb24328641993ac7f66ac095c

Hypothesis: on blocks 23550000 to 23550999, trace-transaction re-execution (predecessors applied) matches the landed
receipt for all up to 95 Aave V3 liquidation transactions that trace-call at the parent block failed on in 80 cases
(result 282), so the paired gap is closed by predecessor state alone.

Metric: /exact_fail_count_trace_transaction. Expected: 0 (range 0 to 2) with replayed_n 95.
Falsifier: exact_fail_count_trace_transaction of 3 or more, or replayed_n differing from 95.
Limit: read-only; no addresses written. Unchanged rerun of tracereplay in trace-transaction mode.

## 2026-11-19T19:54:00Z 285-tracecall-index proposal:9c5defd50c9ff0bdf3c3b6d890b9348cf0feb595404b841db9fb8bc057bd32f3

Hypothesis: in trace-call mode at the parent block on blocks 23550000 to 23550999, every Aave V3 liquidation
transaction at block index 0 (no same-block predecessors) replays exactly, so exact_fail_count_index_zero is 0, while
mismatches concentrate at nonzero index (at least 70 of the 88 nonzero-index transactions).

Metric: /exact_fail_count_index_zero. Expected: 0 (range 0 to 1) with index-zero replayed count above 0.
Falsifier: exact_fail_count_index_zero of 2 or more, or an index-zero replayed count of 0 (sizing null).
Limit: read-only; no addresses written. tracereplay now also writes one row per transaction (index, exact flag) and
the split counts; the replay logic is unchanged.

## 2026-11-19T19:58:24Z 286-tracecall-ctx proposal:9a0663101b3ace91d2d880db1ccc876ea6b893b4885d2ffe2ebf4ab1dd0b8f1d

Hypothesis: passing the landed block header fields (number, time, baseFee, coinbase, gasLimit, random) as the
debug_traceCall block override makes index-zero replays match the receipt, so exact_fail_count_index_zero falls from 7
of 7 in result 285 to at most 3 of 7 on the same blocks 23550000 to 23550999.

Metric: /exact_fail_count_index_zero. Expected: decrease to at most 3 (range 0 to 3).
Falsifier: exact_fail_count_index_zero of 4 or more of replayed_n_index_zero 7, or the node rejects the override
(counted in trace_error_count) so no index-zero replay runs with it.
Limit: read-only; no addresses written. New mode trace-call-ctx; other modes unchanged.

## 2026-11-19T20:07:14Z 288-oracle-pred proposal:21593e124505b08b335d34447ad7d765af3bd0d6c89a07defaa600d1d2c10418

Hypothesis: among the 95 replayed Aave V3 liquidation transactions in blocks 23550000 to 23550999, the trace-call-ctx
mismatch rate is higher for transactions with a same-block oracle-update predecessor (an earlier transaction in the
block whose receipt carries an AnswerUpdated log) than for those without one.

Metric: /exact_fail_count_oracle_predecessor. Expected: at least 9 of the 17 trace-call-ctx mismatches (range 9 to 17).
Falsifier: exact_fail_count_oracle_predecessor of 8 or fewer, or a mismatch rate with an oracle-update predecessor not
higher than the rate without one.
Limit: read-only; no addresses written. New mode trace-call-ctx-oracle replays as trace-call-ctx and adds the flag
from eth_getBlockReceipts; the replay logic is unchanged.

## 2026-11-19T20:14:11Z 290-txindex proposal:1db44ad53e07bb4a9066076d7efaace8397bbd8f9a4bf0dde57d8424137733cc

Hypothesis: if the node accepts the TxIndex option in the debug_traceCall config, tracing at the landed transaction's
index (same-block predecessors applied, header fields from the landed block) leaves at most 8 exact mismatches among the
replayed transactions in blocks 23550000 to 23550999, down from 17 of 95 in result 288.

Metric: /exact_fail_count_txindex (with /txindex_supported). Expected: at most 8 (range 0 to 8) when /txindex_supported is true.
Falsifier: /txindex_supported is false, or exact_fail_count_txindex of 17 or more with replayed_n of at least 90.
Limit: read-only; no addresses written. New mode trace-call-txindex; other modes unchanged.

## 2026-11-19T20:21:56Z 292-txindex-window3 proposal:8b86bbabf8b7145098f5bfda9d66d8fee0bb589e5af53368be3ae121b625ae10

Hypothesis: on blocks 23821000 to 23821999, trace-call-txindex replay of up to 100 landed liquidation transactions gives
exact_fail_count_txindex of 0 with replayed_n of at least 50.

Metric: /exact_fail_count_txindex. Expected: 0 (range 0 to 0), valid only if /replayed_n is at least 50.
Falsifier: exact_fail_count_txindex of 1 or more with replayed_n at least 50; replayed_n below 50 makes the test void.
Limit: read-only; no addresses written. Existing mode, no code change.

## 2026-11-19T20:25:29Z 293-txindex-diff proposal:c26cba261885fb69dc74bf5f12084cd51c3b666eef19ebcc608f237bcdc932fb

Hypothesis: in window 23821000 to 23821999 under trace-call-txindex mode, the mismatching transactions differ in
liquidation log count and not in execution status, so status_diff_count is 0 while log_count_diff_count is at least 1.

Metric: /log_count_diff_count. Expected: 1 (range 1 to 1), with status_diff_count of 0.
Falsifier: status_diff_count of 1 or more among the mismatching rows, or log_count_diff_count of 0 with
exact_fail_count_txindex of 1 or more.
Limit: read-only; no addresses written. per_tx rows gain landed and replayed log counts, status and gas used.

## 2026-11-19T20:30:39Z 294-call-diff proposal:00559d2832450aeb2e2501c50903b8ff21471df81180b855aee185111e1b8057

Hypothesis: in window 23821000 to 23821999 under mode trace-call-txindex-calldiff, every txIndex-replay mismatching
transaction has a located first divergence in its call tree between the landed debug_traceTransaction (callTracer) and the
txIndex traceCall replay, so divergence_located_count equals exact_fail_count_txindex.

Metric: /divergence_located_count. Expected: 1 (range 1 to 1), with exact_fail_count_txindex 1 and replayed_n 13.
Falsifier: divergence_located_count of 0 while exact_fail_count_txindex is 1 or more (call trees identical), or a trace error
on the mismatching transaction.
Limit: read-only; no addresses written. New mode trace-call-txindex-calldiff; other modes unchanged.

## 2026-11-19T20:35:30Z 295-calldiff-class proposal:3638ac073b10536c633512c9cab064fecf75693efbbe3c57512058c606726988

Hypothesis: in window 23821000 to 23821999 the single txIndex replay mismatch (located at depth 2, field value) is a
value-only divergence: same call type, target and input selector in landed and replayed traces, differing only in value.

Metric: /divergence_value_only_count. Expected: 1 (range 1 to 1), with divergence_target_or_selector_diff_count 0 and
divergence_classified_count equal to exact_fail_count_txindex.
Falsifier: divergence_value_only_count of 0 (target, selector or call type differs), or divergence_classified_count of 0.
Limit: read-only; no addresses written. Existing calldiff mode gains classification fields.

## 2026-11-19T20:45:26Z 298-txindex-summary proposal:daef0aab6a968bf7f0e0a3dd2694d71b6c2a6f5d9e142147b2b0c353b904a12f

Hypothesis: in blocks 23725000 to 23729999 under trace-call-txindex with max-txs 100 and summary-only output,
replayed_n is at least 50 and exact_fail_count_txindex is at most 3, so the txIndex replay mismatch rate stays at or
below 3 percent as in the earlier windows (0 of 95 and 1 of 13).

Metric: /exact_fail_count_txindex. Expected: at most 3 (range 0 to 3) with replayed_n at least 50.
Falsifier: exact_fail_count_txindex of 4 or more, or replayed_n below 50 (vacuous run).
Limit: read-only; no addresses written. New flag --summary-only keeps only mismatching per_tx rows, capped at 10.

## 2026-11-19T20:54:34Z 300-txindex-stress proposal:52085111f73bfb8fe8bd7df042fe1e2070c392e4272b3f9f03f1d28306da17f1

Hypothesis: in blocks 23549000 to 23553999 under trace-call-txindex with max-txs 100 and summary-only output,
replayed_n is at least 50 and exact_fail_count_txindex is at most 3, so the txIndex replay mismatch rate stays at or
below 3 percent in a dense stress window.

Metric: /exact_fail_count_txindex. Expected: at most 3 (range 0 to 3) with replayed_n at least 50.
Falsifier: exact_fail_count_txindex of 4 or more, or replayed_n below 50 (vacuous run).
Limit: read-only; no code change; uses the existing --summary-only flag.

## 2026-11-19T21:10:57Z 305-txindex-late proposal:43b7b22704d5fc94a194b8256665164b70932df353744781525167135f45fa5d

Hypothesis: on Ethereum blocks 25500000 to 25504999, replaying landed liquidation transactions at their transaction
index with traceCall reproduces the landed liquidation log count in at least 99 of every 100 replayed transactions
(exact_fail_count_txindex at most 1 with replayed_n at least 50).

Metric: /exact_fail_count_txindex. Expected: at most 1 (range 0 to 1) with replayed_n at least 50.
Falsifier: exact_fail_count_txindex of 2 or more with replayed_n of at least 50, or replayed_n below 50 (vacuous, not supportive).
Limit: read-only; no code change; existing --summary-only flag.

## 2026-11-23T19:08:30Z 313-v4census proposal:64ebe58fee2c149abfb5578e37de96a90459bda02331de7732d11db0a46184d9

Hypothesis: over Ethereum blocks 26160000 to 26209999, the Aave V4 Spokes listed in AaveV4EthereumSpokes at aave-address-book
commit 967c772c19c63c8728c2a3bcf34f6bd5b0ff9abe (oracle, treasury and tokenization constants excluded by name) emit at least one
ISpoke LiquidationCall log, so a V4 liquidation census is non-empty.

Metric: /v4_liquidation_event_count. Expected: at least 1 (range 1 to 100000).
Falsifier: v4_liquidation_event_count equals 0 over the pinned range, or the Spoke list cannot be read from the pinned commit (then spoke_count is 0 and the run reports it).
Limit: read-only eth_getLogs; endpoint taken from the Chainstack platform API at run time and never printed or written.

## 2026-11-23T19:13:30Z 314-v4census-wide proposal:dc33e396ba90d66266efe30daf1dc1e2476f960950a4178985553c2e89644cef

Hypothesis: over Ethereum blocks 24500000 to 26209999, the Aave V4 Spokes listed in AaveV4EthereumSpokes at aave-address-book
commit 967c772c19c63c8728c2a3bcf34f6bd5b0ff9abe emit at least one ISpoke LiquidationCall log, and the count of all logs from
the same Spokes (any topic0) is above zero, so the zero in result 313 reflects a quiet window and not a wrong Spoke list.

Metric: /v4_liquidation_event_count. Expected: at least 1 (range 1 to 100000), with /spoke_all_log_count at least 1.
Falsifier: v4_liquidation_event_count equals 0 over the pinned range while spoke_all_log_count is above 0 (quiet or wrong topic),
or spoke_all_log_count equals 0 (wrong address list).
Limit: read-only eth_getLogs; the new --liveness flag fetches every log from the Spokes and counts the LiquidationCall topic0
locally; endpoint from the Chainstack platform API at run time, never printed or written.

## 2026-11-23T19:22:51Z 315-v4payment proposal:73027d11846f0511d27fe363b99c726a0a1ea7d0320b24075d71efb727a72b6a

Hypothesis: among distinct Aave V4 liquidation transactions in Ethereum blocks 24500000 to 26209999 (address-book commit
967c772c19c63c8728c2a3bcf34f6bd5b0ff9abe), the share paying exactly zero priority fee (effectiveGasPrice equal to the block base fee)
lies between 0.40 and 0.65, the same band as the Aave V3, Spark and Morpho Blue share of 0.517 in results/summary.json.

Metric: /v4_zero_priority_fee_share. Expected: within 0.40 to 0.65.
Falsifier: a share below 0.40 or above 0.65 over the distinct V4 liquidation transactions.
Limit: read-only eth_getLogs, eth_getTransactionReceipt and eth_getBlockByNumber; visible priority fee is a lower bound on the
builder payment (coinbase transfers not counted); endpoint from the Chainstack platform API at run time, never printed or written.

## 2026-11-23T19:27:38Z 316-v4oracle proposal:372c83251241e20071fc2eab8632f3f4357c06a615862790adefb1b567eb7003

Hypothesis: over Ethereum blocks 24500000 to 26209999 (address-book commit 967c772c19c63c8728c2a3bcf34f6bd5b0ff9abe), the share of
distinct Aave V4 liquidation landing blocks that contain at least one Chainlink AnswerUpdated log from any feed lies between 0.40 and
0.80, the same direction as the same-block share of 0.608 in results/summary.json, and exceeds the share in 1000 seeded baseline
blocks from the same range by an enrichment ratio of at least 1.5.

Metric: /v4_same_block_oracle_share. Expected: within 0.40 to 0.80, with /enrichment_vs_baseline at least 1.5.
Falsifier: a share below 0.40 or above 0.80, or an enrichment below 1.5, over the distinct V4 liquidation blocks.
Limit: read-only eth_getLogs; baseline blocks are drawn with seed 42 from the range excluding liquidation blocks; any-feed adjacency,
not the position's own feed; endpoint from the Chainstack platform API at run time, never printed or written.

## 2026-11-23T19:35:39Z 318-v4oraclefee proposal:a401c5a692b99ac7ad599a1717373d62167939f5b4aeaf33a3b9a29eccb5829c

Hypothesis: among distinct Aave V4 liquidation transactions in Ethereum blocks 24500000 to 26209999 (address-book commit
967c772c19c63c8728c2a3bcf34f6bd5b0ff9abe) paying exactly zero priority fee, the share landing in a block with a Chainlink
AnswerUpdated log is at least 0.55, and that share is not lower than the share among transactions paying a positive priority fee.

Metric: /oracle_adjacent_share_zero_fee. Expected: at least 0.55 (range 0.55 to 0.9).
Falsifier: oracle_adjacent_share_zero_fee below 0.55, or its Wilson upper bound below the paid-group share's Wilson lower bound.
Limit: read-only eth_getLogs, eth_getTransactionReceipt and eth_getBlockByNumber; visible priority fee is a lower bound on the
builder payment; any-feed adjacency; endpoint from the Chainstack platform API at run time, never printed or written.

## 2026-11-23T19:41:23Z 319-v4updategap proposal:858136ac53ad867fc8cbe955db40a0268afc382d1dd9be321c9f7ba7cf9c0a75

Hypothesis: among distinct Aave V4 liquidation transactions in Ethereum blocks 24500000 to 26209999 (address-book commit
967c772c19c63c8728c2a3bcf34f6bd5b0ff9abe) that land in a block with a Chainlink AnswerUpdated log at a lower transaction index
(any feed, last such log), at least half land within 3 transaction positions after that log (distance 1 to 3), the backrun
signature; the Wilson 95% lower bound of that share is at least 0.50.

Metric: /liquidation_within_k_after_update_share (k = 3). Expected: at least 0.50 (range 0.5 to 1.0).
Falsifier: the Wilson 95% lower bound of the share is below 0.50, or the update transaction index cannot be read from logs.
Limit: read-only eth_getLogs; the update log is any Chainlink feed, not the position's own feed, and "Spoke oracle update" is read as
this any-feed AnswerUpdated log, as in results 316 and 318; the denominator excludes liquidations with no preceding update in their
block (counted separately); the liquidation index is that of the first V4 LiquidationCall log per transaction; the endpoint comes
from the Chainstack platform API at run time, never printed or written.

## 2026-11-23T19:49:29Z 320-v4reexec proposal:e2ad6e5a8938d0622cefcb36fd1b514a402530f5ffa44e07290bf3eb2800e7ae

Hypothesis: replaying the distinct Aave V4 liquidation transactions of results 314 and 319 (Ethereum blocks 24500000 to 26209999,
address-book commit 967c772c19c63c8728c2a3bcf34f6bd5b0ff9abe) with debug_traceCall at the parent block state yields a different
debtAmountRestored from the landed event in at least 0.20 of the transactions that replay successfully, and the mismatch share is
higher for transactions at block index above 0 than at index 0.

Metric: /reexec_amount_mismatch_share. Expected: at least 0.20 (range 0.20 to 1.0).
Falsifier: the mismatch share among successfully replayed transactions is below 0.20 (Wilson upper bound below 0.20), or the share at
index 0 is not lower than at index above 0.
Limit: read-only eth_getLogs, eth_getTransactionReceipt, eth_getTransactionByHash, eth_getBlockByNumber and debug_traceCall; replay at the
parent block state ignores earlier same-block transactions by construction; a transaction is compared by the ordered list of
debtAmountRestored values in its V4 LiquidationCall logs; failed replays are counted separately and excluded from the denominator;
the endpoint comes from the Chainstack platform API at run time, never printed or written.

## 2026-11-23T19:55:20Z 321-v4replayfail proposal:1c9cd08f7e82f662a9f2b8b7d9fd93e6cf2b95ede4abc358df31b10024315891

Hypothesis: among the failed V4 replays in blocks 24500000 to 26209999 (address-book commit 967c772c19c63c8728c2a3bcf34f6bd5b0ff9abe), the share
classified as an execution revert (the position is not yet liquidatable at the parent block state, because an earlier transaction in the same
block created the opportunity) is at least 0.50, and the total failed count reproduces result 320 at 157.

Metric: /revert_share_of_failed. Expected: at least 0.50 and failed_replay_count equal to 157 (range 0.5 to 1.0).
Falsifier: the Wilson 95% upper bound of revert_share_of_failed is below 0.50, or failed_replay_count differs from 157, or the error field cannot
be classified for more than half of the failures (reported as such).
Limit: read-only eth_getLogs, eth_getTransactionByHash and debug_traceCall at the parent block; classes are by substring of the node's error
message (revert, state-unavailable, unsupported-method, other); the revert class says the replay reverted, and the same-block-cause reading is an
interpretation not tested here; error text is not written to the results; the endpoint comes from the Chainstack platform API at run time,
never printed or written.

## 2026-11-23T20:00:30Z 322-v4revertflip proposal:f8a412cbe1dd5f97d21c9946a1a13191dd47f807a946c1c2151f2dda82e9f963

Hypothesis: among the 153 V4 replays that revert at parent-block state in blocks 24500000 to 26209999 (result 321, address-book commit
967c772c19c63c8728c2a3bcf34f6bd5b0ff9abe), at least half no longer revert when replayed at the landed block with the earlier transactions
of the same block applied (txIndex).

Metric: /revert_flip_to_success_share. Expected: at least 0.50 (range 0.5 to 1.0), with the reverting count reproducing 153.
Falsifier: the Wilson 95% upper bound of revert_flip_to_success_share is below 0.50, or the node cannot trace at a transaction index
(reported as an abstain count).
Limit: read-only eth_getLogs, eth_getTransactionByHash and debug_traceCall; the revert class is by substring of the node's error message as
in result 321; a flip to success says the call no longer reverts, not that its output matches the landed event; the endpoint comes from the
Chainstack platform API at run time, never printed or written.

## 2026-11-23T20:05:26Z 323-v4reexecpred proposal:83d54dafcf5eccf3573bf83cb6ce9cc509e6f41066b88b517bc474276a757f54

Hypothesis: with same-block predecessors applied (debug_traceCall at the landed block with txIndex), the share of replayed V4 liquidation
transactions whose ordered debtAmountRestored values differ from the landed events is lower than the 0.6977 of result 320, with the upper
Wilson bound of the new share below 0.6977, on blocks 24500000 to 26209999 (address-book commit 967c772c19c63c8728c2a3bcf34f6bd5b0ff9abe).

Metric: /reexec_amount_mismatch_share_with_predecessors. Expected: decrease to at most 0.40 (range 0.0 to 0.40).
Falsifier: a mismatch share whose Wilson 95% interval contains 0.6977 or lies above it.
Limit: read-only eth_getLogs, eth_getTransactionByHash and debug_traceCall; replays that fail are counted separately and excluded from
the denominator; the endpoint comes from the Chainstack platform API at run time, never printed or written.

## 2026-11-23T20:16:00Z 325-v4dustrule proposal:02c22ad4c4c86160f7f063a4f6cd1e2219fea344c38c0566a0cf0a4200d2b409

Hypothesis: on blocks 24500000 to 26209999 (address-book commit 967c772c19c63c8728c2a3bcf34f6bd5b0ff9abe), at most 5 of the V4 liquidation
transactions of result 314 leave nonzero remaining debt in the target reserve below the documented dust threshold, because the documented rule
forbids such remainders.

Metric: /dust_rule_violation_count. Expected: at most 5 (range 0 to 5).
Falsifier: dust_rule_violation_count is 6 or more of the priced liquidations, or debt cannot be read and priced at a pinned block for more than
half of the 243 liquidations (reported as an abstain count and the run counts as null).
Limit: read-only eth_getLogs, eth_call at the landing block; remaining debt is read from the Spoke after the landing block, priced from the Spoke
oracle at that block; the threshold is read from the Spoke or its documented constant at run time; the endpoint comes from the Chainstack platform
API at run time, never printed or written.

## 2026-11-23T20:20:56Z 326-v4dustpre proposal:19aa70ba37d51eb68318a6301e0d3de85f0d647c80b11ebc446ed73757826049

Hypothesis: of the V4 liquidation transactions of result 325 that leave nonzero debt valued below the documented 1000 USD threshold (15 in result 325),
at least 10 had a debt value at or above 1000 USD at the block before landing, on blocks 24500000 to 26209999 (address-book commit
967c772c19c63c8728c2a3bcf34f6bd5b0ff9abe), so the rule does not explain them by pre-existing dust.

Metric: /subthreshold_with_abovethreshold_predebt_count. Expected: at least 10 (range 10 to 15).
Falsifier: a count below 10, or an abstention that prevents classifying the sub-threshold transactions.
Limit: read-only eth_getLogs and eth_call at blocks N-1 and N; debt is priced from the Spoke oracle at the same block; the endpoint comes from the
Chainstack platform API at run time, never printed or written.

## 2026-11-23T20:32:30Z 328-v4bonuscurve100 proposal:0722750d127c42863ac62e0e74d329b82601816d9064ed3b8cf9f136efe92fa1

Hypothesis: on blocks 24500000 to 26209999 (address-book commit 967c772c19c63c8728c2a3bcf34f6bd5b0ff9abe), at most 5 percent of priced V4 liquidation
transactions have a landed bonus (seized collateral value over repaid debt value, minus one) differing from the Spoke's health-factor bonus by more
than 100 basis points; every unpriced transaction is counted under the reason it could not be priced.

Metric: /bonus_outside_100bps_count. Expected: at most 12 of 243 V4 liquidation transactions (range 0 to 12).
Falsifier: a count above 12 among priced transactions, or a priced count below 200 (then the check is underpowered and the hypothesis is not supported).
Limit: read-only eth_getLogs and eth_call at block N-1; values priced from the Spoke oracle at that block; the endpoint comes from the Chainstack
platform API at run time, never printed or written.

## 2026-11-23T20:38:24Z 329-v4bonuscollrid proposal:68835baa31b414d17942cd024aa8b19d96109dfc274b35228b59425dbebaea0c

Hypothesis: on blocks 24500000 to 26209999 (address-book commit 967c772c19c63c8728c2a3bcf34f6bd5b0ff9abe), querying the Spoke liquidation bonus with
the collateral reserve id instead of the debt reserve id leaves at most 20 of the 243 V4 liquidation transactions unpriced, where result 328 left 131
unpriced as bonus unreadable.

Metric: /abstain_tx_count. Expected: falls from 131 to at most 20 (range 0 to 20).
Falsifier: an abstain_tx_count above 20, or a bonus-unreadable reason count above 20.
Limit: read-only eth_getLogs and eth_call at block N-1; the endpoint comes from the Chainstack platform API at run time, never printed or written.

## 2026-11-23T20:48:45Z 331-v4bonusdiag proposal:1d4fbcd613a26a17afc51daa032403250a3df8ca7b6cdd70c46b102aa8cdcd19

Hypothesis: on blocks 24500000 to 26209999 (address-book commit 967c772c19c63c8728c2a3bcf34f6bd5b0ff9abe), at least 120 of the 131 V4 liquidation
transactions left unreadable in result 329 fail the Spoke bonus call with an on-chain revert or an empty return rather than a timeout or transport
error, so the abstentions come from the call itself and not from the node. Failures are also counted per spoke.

Metric: /bonus_call_revert_or_empty_count. Expected: at least 120 of 131 (range 120 to 131).
Falsifier: a revert-or-empty count below 120, a timeout-or-transport count above 11, or an unreadable total different from 131.
Limit: read-only eth_getLogs and eth_call at block N-1; the endpoint comes from the Chainstack platform API at run time, never printed or written.

## 2026-11-23T20:53:27Z 332-v4bonusdebtrid proposal:c84b4c4837f36e0a31578b1a2763562b751dc9fec0c390b3099f308b5a2a36c6

Hypothesis: on blocks 24500000 to 26209999 (address-book commit 967c772c19c63c8728c2a3bcf34f6bd5b0ff9abe), re-issuing the Spoke bonus call with the
debt reserve id makes at least 100 of the 131 transactions unreadable in result 331 readable, so the collateral reserve id was the wrong argument for those spokes.

Metric: /retry_readable_tx_count. Expected: rises from 0 to at least 100 of 131 (range 100 to 131).
Falsifier: a retry_readable_tx_count below 100 of the 131 unreadable transactions in result 331.
Limit: read-only eth_getLogs and eth_call at block N-1; the endpoint comes from the Chainstack platform API at run time, never printed or written.

## 2026-11-23T20:58:10Z 333-v4bonuspos proposal:08aa9d2841a77825587ddb4063860f723547e396efa7ae65f4b138368f38fbcf

Hypothesis: on blocks 24500000 to 26209999 (address-book commit 967c772c19c63c8728c2a3bcf34f6bd5b0ff9abe), at least 100 of the 131 V4 liquidation
transactions whose Spoke bonus call reverts at N-1 in result 332 still show a non-zero borrower collateral position in the liquidated reserve at N-1, so the reverts are not explained by a position already closed.

Metric: /position_nonzero_tx_count. Expected: at least 100 of 131 (range 100 to 131).
Falsifier: a position_nonzero_tx_count below 100 of the 131 reverting transactions, or an unreadable position count above 31.
Limit: read-only eth_getLogs and eth_call at block N-1; the endpoint comes from the Chainstack platform API at run time, never printed or written.

## 2026-11-23T21:12:03Z 336-v4bonuscfg proposal:827d7c66ef11816c7aa845dd27362702e2d664f50cd4899fdafebaf28604c0b3

Hypothesis: on blocks 24500000 to 26209999 (address-book commit 967c772c19c63c8728c2a3bcf34f6bd5b0ff9abe), when the dynamic-reserve-config getter is keyed by the dynamic config key read from the borrower's own position at N-1, it returns data for at least 125 of the 131 V4 liquidation transactions whose Spoke bonus call reverts in result 332. The liquidation-config getter is also counted. Both are counted per spoke.

Metric: /dynamic_config_answering_tx_count. Expected: at least 125 of 131 (range 125 to 131).
Falsifier: a dynamic_config_answering_tx_count below 125 of the 131 reverting transactions, or an unreadable (timeout or transport) count above 6.
Limit: read-only eth_getLogs and eth_call at block N-1; the endpoint comes from the Chainstack platform API at run time, never printed or written.

## 2026-11-23T21:17:06Z 337-v4bonuscfg32 proposal:c3d488e6c0eea1b6a9e4d5285e9929a88b3c5cf4abe2306c6198e14e1a28b0a7

Hypothesis: on blocks 24500000 to 26209999 (address-book commit 967c772c19c63c8728c2a3bcf34f6bd5b0ff9abe), calling the dynamic-reserve-config getter with a uint32 dynamic config key read from the borrower's position at N-1 returns data for at least 125 of the 131 V4 liquidation transactions whose Spoke bonus call reverts in result 332, while the uint24 control still answers for 0, so the failure in result 336 was the selector width and not state.

Metric: /dynamic_config_u32_answering_tx_count. Expected: rises from 0 (result 336) to at least 125 of 131 (range 125 to 131).
Falsifier: a dynamic_config_u32_answering_tx_count below 125 of the 131 reverting transactions, or an unreadable (timeout or transport) count above 6.
Limit: read-only eth_getLogs and eth_call at block N-1; the endpoint comes from the Chainstack platform API at run time, never printed or written.

## 2026-11-23T21:22:35Z 338-v4bonuscmp proposal:8bc10fd0fe574ec6310b6b216267725e479fadb862c8dc8d284dba6a1731b558

Hypothesis: on blocks 24500000 to 26209999 (address-book commit 967c772c19c63c8728c2a3bcf34f6bd5b0ff9abe), for at least 120 of the 131 V4 liquidation transactions whose Spoke bonus call reverts in result 332, the landed bonus in basis points does not exceed the configured maxLiquidationBonus read at N-1 (uint32-keyed dynamic config) by more than 100 bps, so the landed bonus is consistent with the configured cap.

Metric: /landed_within_max_bonus_tx_count. Expected: at least 120 of 131 (range 120 to 131).
Falsifier: a landed_within_max_bonus_tx_count below 120 of the 131 reverting transactions, or an unreadable count above 11.
Limit: read-only eth_getLogs and eth_call at block N-1; the endpoint comes from the Chainstack platform API at run time, never printed or written.

## 2026-11-23T21:45:24Z 339-v4bonusabove proposal:4a29a991aeecd67da25e37eeeb4f68f8249c7672f6ae84b1b6ab15f26801c52b

Hypothesis: on blocks 24500000 to 26209999 (address-book commit 967c772c19c63c8728c2a3bcf34f6bd5b0ff9abe), for at least 1 of the 2 V4 liquidation transactions that result 338 counts above the configured maximum bonus, the landed bonus in basis points exceeds the maxLiquidationBonus read at the landing block N by no more than 100 bps, so the N-1 read of result 338 (oracle price and config key) explains that case.

Metric: /above_max_explained_by_landing_block_tx_count. Expected: at least 1 of 2 (range 1 to 2).
Falsifier: an above_max_explained_by_landing_block_tx_count equal to 0 of the 2 above-max transactions, or an above-max transaction set of a size other than 2 under the N-1 read.
Limit: read-only eth_getLogs and eth_call at blocks N-1 and N; the endpoint comes from the Chainstack platform API at run time, never printed or written.

## 2026-11-23T21:52:57Z 340-v4bonuschg proposal:70808b3d88c3466472299ceab9c7274bccc558faddeef0a6afce8c7b2f42a693

Hypothesis: on blocks 24500000 to 26209999 (address-book commit 967c772c19c63c8728c2a3bcf34f6bd5b0ff9abe), the configured maxLiquidationBonus read at the landing block N differs from the one read at N-1 for at least 2 of the 243 V4 liquidation transactions, covering the 2 above-max cases of result 339.

Metric: /config_changed_between_n_minus_1_and_landing_tx_count. Expected: at least 2 of 243 (range 2 to 243).
Falsifier: a config_changed_between_n_minus_1_and_landing_tx_count below 2 of 243, or an unreadable count above 24.
Limit: read-only eth_getLogs and eth_call at blocks N-1 and N; the endpoint comes from the Chainstack platform API at run time, never printed or written.

## 2026-11-23T21:58:28Z 341-v4bonusprice proposal:dd3525506649dd4851d8e46baf822255fd61a65eb969d1153842095c9ce9fe70

Hypothesis: on blocks 24500000 to 26209999 (address-book commit 967c772c19c63c8728c2a3bcf34f6bd5b0ff9abe), for at least 1 of the 2 above-max V4 liquidation transactions of result 339, the oracle price of the collateral or the debt reserve read at the landing block N differs from the one read at N-1, so a same-block price move rather than a config change explains the N-1 versus N discrepancy.

Metric: /above_max_price_changed_tx_count. Expected: at least 1 of 2 (range 1 to 2).
Falsifier: an above_max_price_changed_tx_count of 0 of the 2 above-max transactions, an above-max set of a size other than 2, or an unreadable count above 0.
Limit: read-only eth_getLogs and eth_call at blocks N-1 and N; the endpoint comes from the Chainstack platform API at run time, never printed or written.

## 2026-11-23T22:09:31Z 343-v4pricerate proposal:09afe746d260f51538c3878ca270d77ecc69c8eb38e1689768504560555a3b37

Hypothesis: on blocks 24500000 to 26209999 (address-book commit 967c772c19c63c8728c2a3bcf34f6bd5b0ff9abe), the oracle price of the collateral or debt reserve differs between N-1 and N for at least 65 of the within-max reverting V4 liquidation transactions, so a changed price is common among the controls and the 2 of 2 of result 341 is not specific to the above-max cases.

Metric: /within_max_price_changed_tx_count. Expected: at least 65 (range 65 to 131).
Falsifier: a within_max_price_changed_tx_count below 65, or an unreadable count above 13.
Limit: read-only eth_getLogs and eth_call at blocks N-1 and N; the endpoint comes from the Chainstack platform API at run time, never printed or written.

## 2026-11-23T22:20:22Z 344-basedenim proposal:82c7509aa5e879407780f74267f8a7899b401d875519da5447c41dca5e6a202b

Hypothesis: on Base mainnet blocks 53556127 to 53556426, at least 100 of the 300 blocks contain only deposit-type (0x7e) system transactions, and block 53556127 is the first block whose timestamp equals that of its successor, so a full node confirms the single-outlet report of 146 of the first 300 blocks being system-only within a wide band.

Metric: /first_300_blocks_system_only_count. Expected: at least 100 of 300 (range 100 to 300).
Falsifier: a first_300_blocks_system_only_count below 100, a first block sharing its timestamp with its successor other than 53556127, or an unreadable block count above 0.
Limit: read-only eth_getBlockByNumber on the Chainstack Base node; the endpoint comes from the Chainstack platform API at run time, never printed or written.

## 2026-11-23T22:25:15Z 345-v4revertadj proposal:1fd9dab641d2fa7a5330124c9b6ee961171ecd9e0bd6446be1cc6057f0d75e02

Hypothesis: on blocks 24500000 to 26209999 (address-book commit 967c772c19c63c8728c2a3bcf34f6bd5b0ff9abe), at least 66 of the 131 V4 liquidation transactions whose Spoke bonus call reverts at N-1 have a Chainlink AnswerUpdated log at a lower transaction index in the same block, so the N-1 versus N price change of result 343 coincides with an in-block oracle update ahead of the liquidation.

Metric: /reverting_with_preceding_update_tx_count. Expected: at least 66 of 131 (range 66 to 131).
Falsifier: a reverting_with_preceding_update_tx_count below 66, a reverting_tx_count other than 131, or an unreadable count above 13.
Limit: read-only eth_getLogs and eth_call at block N-1; the endpoint comes from the Chainstack platform API at run time, never printed or written.

## 2026-11-23T22:32:39Z 346-basedenim-later proposal:1bd35e39a82d25c27fccd3481970890c2ebe87d5e6d81c25382267cb03d0ed61

Hypothesis: on Base mainnet blocks 53988127 to 53988426, at least 100 of the 300 blocks contain only deposit-type (0x7e) system transactions, so the system-only share of result 344 persists one day after its window.

Metric: /first_300_blocks_system_only_count. Expected: at least 100 of 300 (range 100 to 300).
Falsifier: a first_300_blocks_system_only_count below 100, or an unreadable block count above 0.
Limit: read-only eth_getBlockByNumber on the Chainstack Base node; the endpoint comes from the Chainstack platform API at run time, never printed or written.

## 2026-11-23T22:37:00Z 347-basedenim-third proposal:6081d80aaa632d0351e9bc5fe93a45cb68fbc3d9d5ad656f27f504f13464fc4e

Hypothesis: on Base mainnet blocks 54420127 to 54420426, fewer than 10 of the 300 blocks contain only deposit-type (0x7e) system transactions, so the near-zero system-only share of result 346 persists in a third window and the 146 of 300 of result 344 was confined to the first window.

Metric: /first_300_blocks_system_only_count. Expected: below 10 of 300 (range 0 to 9).
Falsifier: a first_300_blocks_system_only_count of 10 or more, or an unreadable block count above 0.
Limit: read-only eth_getBlockByNumber on the Chainstack Base node; the endpoint comes from the Chainstack platform API at run time, never printed or written.
