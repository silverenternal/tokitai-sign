# Topology-Constrained Temporal Glyph Assignment

## Purpose

`topology-dp` is the proposed algorithmic mode. It upgrades the earlier
path-indexed history cache into an explicit topology-constrained assignment
step over ordered path samples.

## Pseudocode

```text
Input:
  visible path samples V_t
  previous path-indexed state S_{t-1}
  frame box path topology P_t
  glyph alphabet A

For each visible sample i in path order:
  C_i = {
    current candidate path identity,
    same path index i,
    previous neighbor i - 1,
    next neighbor i + 1
  }

  For each candidate c in C_i:
    temporal_cost = circular_distance(S_{t-1}[i].path_index, c)
    local_cost = circular_distance(i, c)
    topology_cost = disagreement with already assigned neighbors
    corner_cost = glyph(P_t, c) changes corner identity

    total_cost(c) =
      1.8 * temporal_cost
    + 0.55 * local_cost
    + 2.4 * topology_cost
    + corner_cost

  assign sample i to argmin_c total_cost(c)

Update S_t with the selected path identities.
Project selected identities into terminal cells.
```

## Baselines

| Mode | Meaning | Prior-work analogue |
| --- | --- | --- |
| `off` | Per-frame glyph choice with no temporal identity. | Static vector-to-cell or glyph rasterization. |
| `screen-cell` | History keyed by terminal cell coordinate. | Screen-space TAA or dirty-cell state. |
| `path` | History keyed by path index with local hold/decay. | Path-indexed temporal cache. |
| `topology-dp` | Path-indexed history plus topology-constrained assignment. | Proposed method. |

## Claim Boundary

The novelty is not that dynamic programming or temporal history exists. The
claim is that the assignment object is a moving line primitive represented by a
discrete character-cell glyph alphabet, and the state is optimized along path
topology before projection to terminal cells.

## Complexity and Parameters

The implemented assignment is local over ordered path samples. For `n` visible
path samples and `k` candidate identities per sample, the assignment cost is
`O(n*k)`. The current implementation uses `k = 4`, so the expected scaling is
linear in path length.

The current default weights are empirical:

```text
temporal_cost = 1.8
local_cost = 0.55
topology_cost = 2.4
corner_cost = 1.0 when the candidate changes corner identity
```

These values should not be presented as universal constants. The artifact now
includes `table_8_weight_sensitivity.csv`, which reports raw and normalized
weight variants and includes Unicode, ASCII, and degraded glyph-alphabet proxy
rows. The intended claim is bounded: the default lies in a useful stable region
for canonical fixtures and the proxy alphabets, while aggressive perturbations
can still degrade toward the path-cache baseline.

## Failure Modes

`topology-dp` assumes that adjacent frames preserve enough path correspondence
to define candidate identities. The current stress results keep high-speed
motion as an explicit failure-boundary case: once the primitive moves far enough
that correspondence is not recoverable, topology-dp sets a correspondence-loss
flag and can degrade to unstable corner identity. It is expected to degrade, not
remain perfect, under:

- motion faster than the line's recoverable cell correspondence;
- instantaneous topology breaks and reconnects;
- multiple line primitives crossing or competing for the same cell;
- glyph alphabets or fonts with unstable stroke/corner semantics.

The artifact reports these boundaries in `table_7_stress_topology_metrics.csv`
and `figure_6_stress_degradation.png`. Runtime scaling is reported in
`table_9_runtime_complexity.csv` and `figure_8_runtime_scaling.png`.
End-to-end release-frame timing is reported in
`table_10_integrated_runtime.csv`; the current evidence reports release
frame-build time and topology overhead without terminal I/O. The full-visual
path supports a bounded 60 FPS frame-build claim on the current machine. The
low-latency topology-dp path reported in `table_15_runtime_degraded_profile.csv`
is now evaluated against a 1200 FPS frame-build budget in
`table_19_runtime_budget_ladder.csv`, but still not a terminal-I/O or display
refresh guarantee.
