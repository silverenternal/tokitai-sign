# Formal Problem Definition

## Problem

Given a moving line primitive and a terminal character grid, reconstruct a
temporally coherent sequence of discrete line glyphs under a finite update
budget.

The problem is distinct from pixel temporal antialiasing because the output is a
symbolic glyph assignment, and it is distinct from dirty rendering because the
objective is not merely to minimize writes.

## Inputs

- A time-indexed line primitive trajectory `L_t`.
- A terminal grid `G = W x H`.
- A topology-sensitive glyph alphabet `A`, for example horizontal strokes,
  vertical strokes, and corner glyphs.
- A prior reconstruction state `S_{t-1}`.
- A dirty-cell budget `B_t`.

The line primitive is sampled into an ordered path:

```text
P_t = (p_0, p_1, ..., p_n)
```

Each path sample can project into one or more grid cells:

```text
pi_t: P_t -> G
```

## Outputs

- A glyph assignment `g_t: G -> A union empty`.
- A visibility or alpha assignment `a_t: G -> [0, 1]`.
- A dirty update set `D_t subset G`.
- A reconstruction state `S_t` for the next frame.

## Objective

The target reconstruction minimizes:

```text
E = E_coverage
  + lambda_id * E_identity
  + lambda_topo * E_topology
  + lambda_corner * E_corner
  + lambda_budget * E_budget
```

Subject to:

```text
|D_t| <= B_t
g_t(c) in A union empty
S_t is indexed by path identity, not only by screen cell
```

Where:

- `E_coverage` penalizes missing visible path samples.
- `E_identity` penalizes temporal glyph identity changes.
- `E_topology` penalizes disconnected components and path-order violations.
- `E_corner` penalizes unstable corner orientation.
- `E_budget` penalizes required updates over the ANSI dirty-cell budget.

## Why Screen-Space History Is Insufficient

Screen-space history keys state by `(x, y)`. When a sub-cell line primitive
moves, the same path identity may project to a different terminal cell, and a
new occupant may reuse the previous cell coordinate. This can preserve a stale
screen-cell value while losing the primitive identity that determines whether a
glyph should remain a corner, horizontal stroke, or vertical stroke.

The proposed state is keyed by path identity:

```text
S_t[i] = (glyph_i, alpha_i, path_index_i, age_i)
```

The current cell receives the best glyph identity through path-to-cell
projection and topology-constrained assignment.

## Algorithmic Instance

The implemented `topology-dp` mode uses local dynamic assignment over ordered
path samples. For each visible path sample, candidate glyph identities are
scored by:

- temporal identity cost against prior path assignment;
- local path-distance cost;
- neighbor topology consistency cost;
- corner identity change cost.

The selected assignment is the minimum-cost candidate. This is intentionally
separable from the dirty scheduler so that reconstruction quality can be tested
against no-history, screen-cell history, and path-cache baselines.

For paper-facing topology fixtures, emitted orbit cells carry explicit
`primitive_id` and `path_index` metadata. Path-order violations are computed
from that metadata rather than inferred from row/column screen sorting. A
separate `correspondence_lost` flag marks frames where the recoverable
inter-frame correspondence assumption is violated.

## Metrics

The formal objective maps to these measured quantities:

| Objective term | Metric |
| --- | --- |
| `E_topology` | topology breaks per frame, connected-component instability |
| `E_identity` | glyph flips per second |
| `E_corner` | corner identity flip rate |
| path order | path-order violation rate |
| screen continuity | screen discontinuity rate |
| recoverability | correspondence lost rate |
| budget | dirty-cell pressure and budget violation rate |
| stroke shape | stroke metric difference |

Stress figures use a compact diagnostic index:

```text
stress_degradation_index = max(
  corner_glyph_instability,
  stroke_metric_difference,
  path_order_violation_rate,
  correspondence_lost_rate
)
```

This max-over-failures value is used only for compact stress visualization. The
raw columns remain the primary evidence.

The primary novelty evaluation should report topology metrics before aggregate
visual quality.

## Metric Scope

The evaluation intentionally separates identity metrics from screen-connectivity
diagnostics:

- Path-keyed identity metrics follow `primitive_id` and `path_index`. These are
  the primary evidence for the glyph-identity claim.
- Screen-coordinate identity metrics report changes at fixed terminal cells.
  They are useful controls, but they can mark whole-primitive translation as a
  change even when path identity is preserved.
- Screen-connectivity diagnostics report disconnected visible cell components
  or discontinuities in screen-space order. They can flag sampled or open-curve
  structure even when glyph identity remains stable.
- `correspondence_lost_rate` marks frames outside the recoverable local
  candidate window. It is a boundary detector, not a recovery success.

This is why paper-facing low-latency topology rows include both
`identity_verdict` and `topology_verdict`. A row can have stable glyph identity
while screen-connectivity diagnostics remain partially stable. Such a row
supports the identity claim but should not be used as proof of perfect
screen-space connectivity.

Stress and sensitivity evaluations are required to avoid over-interpreting a
zero-error canonical result. A zero corner-flip rate is evidence for the
controlled fixture, not a guarantee under arbitrary topology changes, extreme
speed, or line crossings.
