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
