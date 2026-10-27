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
