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
