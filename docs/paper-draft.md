# Topology-Constrained Temporal Reconstruction for Discrete Character-Cell Line Glyphs

## Abstract

Moving line primitives are difficult to animate coherently on terminal character-cell displays. After a continuous line is projected to a finite grid, the visible object is no longer a pixel sample or a vector stroke; it is a short sequence of symbolic glyphs whose meanings depend on local path topology. A corner glyph, for example, is visually correct only when its temporal identity, path order, and neighboring stroke directions remain consistent across frames. Existing temporal antialiasing methods reconstruct pixel histories, and terminal dirty renderers optimize changed-cell output, but neither directly models temporal identity for topology-sensitive line glyphs after character-cell projection.

We formulate this setting as topology-constrained temporal reconstruction for discrete character-cell line glyphs. To our knowledge, this is the first formulation that assigns temporal identity along a moving line primitive before projection into a terminal glyph alphabet. We propose `topology-dp`, a local dynamic assignment method that scores candidate glyph identities with temporal identity, local path distance, neighbor topology, and corner identity costs. The implementation records explicit `primitive_id` and `path_index` metadata for paper-facing topology fixtures, allowing path-order violations to be measured from recorded path identity rather than from fixture names or screen-space sorting.

Deterministic experiments compare `topology-dp` with per-frame glyph rasterization, screen-cell history, and path-indexed cache baselines. On canonical real-orbit and glyph-stress fixtures, `topology-dp` eliminates corner-glyph instability and glyph flips while preserving metadata-derived path order. Stress tests expose the method's operating envelope: shuffled path metadata is detected, shape-continuity perturbations separate baselines from the proposed method, and high-speed motion is reported as correspondence loss rather than hidden as a successful reconstruction. A speed-boundary sweep further shows the displacement threshold at which correspondence loss begins. Sensitivity and runtime experiments show bounded robustness across raw and normalized weights, proxy glyph alphabets, and release frame-build timing, including a low-latency topology-dp frame-build profile evaluated against a 1200 FPS frame-build budget. The method is therefore claimed as a bounded topology-constrained reconstruction technique, not as universal flicker elimination under arbitrary motion.

## 1. Introduction

Terminal graphics are often treated as a systems problem: which cells changed, how many ANSI writes can be emitted, or how an image can be approximated with Unicode blocks. Those questions are important for a renderer, but they do not capture the temporal reconstruction problem created by moving symbolic line glyphs.

A line primitive has ordered topology. A terminal frame contains occupied cells. The mapping between the two is many-to-one, discrete, and temporally unstable under sub-cell motion. When a moving corner crosses cell boundaries, a screen-space cache can preserve the previous coordinate while losing the path identity that made that coordinate a corner. A simple path cache improves identity retention, but it does not explicitly optimize neighbor consistency or corner identity under ambiguous projection. The result is a symbolic temporal artifact: corner flips, inconsistent stroke directions, broken path order, and transient topology changes.

This paper studies that artifact directly. The central claim is that moving terminal line glyphs should be reconstructed as topology-indexed symbolic identities, not only as screen cells or per-frame glyph choices. The contributions are:

1. A formal problem definition for topology-constrained temporal reconstruction of moving line primitives represented by a discrete character-cell glyph alphabet.
2. `topology-dp`, a path-topology assignment method that minimizes temporal identity, local path distance, neighbor topology, and corner identity costs.
3. A reproducible evaluation suite with metadata-backed topology metrics, stress fixtures, weight and glyph-alphabet sensitivity, and scoped runtime evidence.

The paper does not claim novelty in temporal antialiasing, dirty-region rendering, terminal image rendering, perceptual color metrics, or glyph topology in general. Dirty scheduling and terminal compatibility are included as artifact context and robustness evidence. The academic contribution is the topology-constrained temporal glyph reconstruction formulation and its evaluation.

## 2. Related Work Boundary

Pixel-space temporal antialiasing and temporal upsampling use reprojection, motion vectors, history accumulation, and history validation to stabilize pixel samples. Those methods motivate the use of temporal history, but their reconstructed object is a pixel color or sample distribution, not a symbolic line glyph whose correctness depends on path topology.

Terminal and TUI renderers, including damage-based and dirty-cell systems, optimize the emission of changed cells. They can reduce output bandwidth, but they do not decide whether a moving path sample should remain a corner, horizontal stroke, vertical stroke, or crossing glyph. In this work, dirty output is a constraint around the reconstruction problem, not the novelty.

Terminal image renderers and ASCII-art methods map images or video frames to characters. They may optimize glyph appearance, contrast, or dithering. The target here is narrower: preserving temporal identity of moving line primitives after projection into a topology-sensitive glyph alphabet. Static or per-frame glyph approximation is therefore a baseline, not the proposed method.

Vector stroke animation and non-photorealistic line coherence work preserve continuous stroke identity in vector or surface domains. This paper addresses a different output constraint: the final representation is a finite terminal grid with discrete glyph choices, where local path identity must be re-assigned after grid projection.

The current prior-art matrix records the closest public and database-style search results. Because authenticated ACM DL, IEEE Xplore, Scopus, Web of Science, and SpringerLink export checks have not all been completed in this artifact, the manuscript should use "to our knowledge" for first-known claims.

Table 24 summarizes baseline positioning. The implemented baselines are not
intended as strawmen: per-frame rasterization represents independent glyph
choice, screen-cell history represents coordinate-keyed temporal history, and
the path cache tests whether path identity alone is sufficient. TUI dirty
renderers, terminal image renderers, pixel TAA, and vector stroke-coherence
methods are treated as adjacent systems because they solve output bandwidth,
image-to-glyph approximation, pixel history, or continuous stroke coherence
rather than path-indexed topology-sensitive glyph assignment.

## 3. Problem Formulation

Let `L_t` be a moving line primitive at time `t`, `G` be a terminal grid, and `A` be a finite glyph alphabet containing horizontal strokes, vertical strokes, corners, and optional crossings. The renderer samples the line into an ordered path:

```text
P_t = (p_0, p_1, ..., p_n)
```

Each path sample may project to a terminal cell:

```text
pi_t: P_t -> G
```

The reconstruction must choose a glyph assignment `g_t`, an opacity or visibility assignment `a_t`, a dirty update set `D_t`, and a next reconstruction state `S_t`. The objective is:

```text
E = E_coverage
  + lambda_id * E_identity
  + lambda_topo * E_topology
  + lambda_corner * E_corner
  + lambda_budget * E_budget
```

subject to finite glyph choices and a dirty-cell budget. The key state is path-indexed:

```text
S_t[i] = (glyph_i, alpha_i, path_index_i, age_i)
```

This differs from screen-space history because the identity follows the moving primitive topology rather than a terminal coordinate. Paper-facing topology fixtures emit `primitive_id`, `path_index`, and `correspondence_lost` metadata. The metadata is used only for evaluation and artifact auditing; the visual reconstruction still operates through glyph assignment.

## 4. Method

The proposed `topology-dp` mode creates visible path samples and evaluates candidate identities for each sample. For path sample `i`, the candidate set is local:

```text
C_i = {current, i, i - 1, i + 1}
```

Each candidate receives:

```text
cost(c) =
  w_t * temporal_identity_cost(c)
+ w_l * local_path_distance(c)
+ w_n * neighbor_topology_cost(c)
+ w_c * corner_identity_cost(c)
```

The selected candidate becomes the glyph identity projected to the terminal cell. The current implementation uses a local ordered dynamic assignment with constant-size candidate sets, so the assignment cost is linear in the number of visible path samples for fixed candidate count.

The baselines are intentionally simple and interpretable. `off` performs per-frame glyph rasterization without temporal identity. `screen-cell` stores history by terminal coordinate, analogous to coordinate-keyed temporal history. `path` stores identity by path index but does not run topology-constrained assignment. `topology-dp` adds the neighbor and corner identity costs needed to stabilize symbolic line glyphs.

When inter-frame displacement exceeds the recoverable candidate window, the method cannot infer reliable correspondence. Rather than treating such frames as successful reconstruction, the stress fixture marks them with `correspondence_lost`. This is a failure-boundary detector, not a high-speed recovery algorithm.

## 5. Metrics

The primary evaluation uses topology-specific metrics before aggregate visual scores:

- `corner_glyph_instability`: corner identity changes across frames.
- `glyph_flips_per_second`: visible orbit glyph changes per second.
- `path_order_violation_rate`: metadata-derived path-order violations.
- `screen_discontinuity_rate`: discontinuity observed in screen-space cell order.
- `topology_breaks_per_frame`: disconnected orbit components per frame.
- `connected_component_instability`: changes in component count.
- `crossing_ambiguity_rate`: ambiguous crossing-cell usage.
- `correspondence_lost_rate`: fraction of orbit cells marked as unrecoverable correspondence.
- `stroke_metric_difference`: adjacent-frame change in stroke descriptor.

For compact visualization, Figure 6 uses:

```text
stress_degradation_index = max(
  corner_glyph_instability,
  stroke_metric_difference,
  path_order_violation_rate,
  correspondence_lost_rate
)
```

This index is a max-over-failures diagnostic for plotting stress fixtures. It is not a learned perceptual score and does not replace the raw metrics in Table 7.

Metric scope is deliberately split. Path-keyed identity metrics follow
`primitive_id` and `path_index` and are the primary evidence for the glyph
identity claim. Screen-coordinate identity is a control that can flag
translation even when path identity is stable. Screen-connectivity diagnostics
measure visible cell continuity and can flag sampled or open-curve structure.
Table 18 therefore reports both `identity_verdict` and `topology_verdict`: the
real-orbit row is glyph-identity stable, while the screen-connectivity
diagnostic remains partially stable. This row is not hidden and should not be
used as a claim of perfect screen-space connectivity.

## 6. Results

### 6.1 Canonical Reconstruction

Table 6 and Figure 5 are the primary novelty results. On real-orbit and glyph-stress fixtures, `topology-dp` reduces `corner_glyph_instability` and `glyph_flips_per_second` to zero, while metadata-derived `path_order_violation_rate` remains zero. The screen-cell and path-cache baselines reduce some flicker but retain nonzero corner instability or glyph flips. This supports the bounded claim that topology-constrained assignment removes canonical symbolic identity flips when inter-frame path correspondence is recoverable.

The result should not be read as a universal guarantee. It is a controlled canonical result under recoverable correspondence and a fixed discrete glyph alphabet.

The fixture set is broader than the canonical orbit and glyph-stress rows.
`shuffled-path`, `high-speed`, `topology-break`, `line-crossing`,
`bounded-jitter`, and `shape-continuity` fixtures are included specifically to
test path metadata corruption, correspondence-window failure, topology
mutation, crossing ambiguity, recoverable perturbation, and shape-continuity
stress. These fixtures are synthetic but serve as boundary probes rather than
additional success-only examples.

### 6.2 Stress Degradation and Failure Boundaries

Table 7 is interpreted raw-metric first. Figure 6 only visualizes the max-over-failures `stress_degradation_index`; it is not the primary evidence. Table 12 provides a one-line interpretation for each stress fixture.

Table 7 evaluates shuffled path metadata, high-speed motion, topology breaks, line crossing, bounded jitter, and shape-continuity perturbations. The canonical row is the recoverable reference. The shuffled-path fixture verifies that path-order metrics are metadata-derived: the corrupted path metadata produces high path-order violation while the canonical fixture remains zero. The topology-break row shows that transient removed cells remain measurable as discontinuity rather than being hidden by the assignment. Line-crossing reports crossing ambiguity rather than claiming universal crossing resolution. Bounded jitter remains inside the recoverable operating region. The shape-continuity fixture separates weaker baselines from `topology-dp` on stroke and topology stability.

The high-speed fixture is the most important limitation. `topology-dp` reports `correspondence_lost_rate = 1.0` and remains unstable in corner identity. This is intentional evidence of the method's boundary: unrecoverable inter-frame displacement is detected, not solved.

Table 11 and Figure 10 isolate this boundary with a displacement sweep. The table is a correspondence-window diagnostic: it reports fixture-level `expected_correspondence_recoverable` and `ground_truth_outside_candidate_window` labels, then records whether each mode reports `correspondence_lost_rate`. The path baseline does not detect outside-window motion; `topology-dp` reports the fixture-level boundary once inter-frame displacement exceeds the local candidate window. This supports a bounded operating-envelope claim, not a high-speed recovery claim.

Table 14 separately controls for translated low-speed motion using path-keyed identity metrics. Screen-coordinate corner flips are intentionally shown as unsuitable for whole-primitive translation: they report instability even when the same path-indexed corner identity is preserved at a new cell. The path-keyed metric shows that `topology-dp` preserves identity inside the candidate window and reports correspondence loss outside it.

### 6.3 Weight and Glyph-Alphabet Sensitivity

Table 8 and Figure 7 report raw and normalized weight variants. Table 13 summarizes tested stability intervals around the default weights, and Table 16/Figure 11 add a denser five-point multiplier grid from 0.5x to 1.5x for each weight axis. The default settings remain stable on canonical fixtures and on Unicode, ASCII, and degraded glyph-alphabet proxies. Aggressive temporal or topology perturbations can still degrade results, so the weights should be presented as bounded relative preferences rather than universal constants. The default weights are selected as balanced identity, local-distance, topology, and corner preferences; they are not tuned to erase stress-boundary failures.

The glyph-alphabet rows are proxy robustness evidence. They show that the canonical result is not only a Unicode box-drawing artifact, but they do not prove font-independent behavior across terminal emulators. The artifact also records one local terminal-layer context, but the outer emulator and visual box-drawing connectivity are not certified from a screenshot. Broader real terminal/font capture remains recommended before submission.

### 6.4 Runtime Scope

Table 9 reports assignment microbenchmarks and supports the linear fixed-candidate implementation claim. Table 10 reports full-visual release frame-build timing and a topology-overhead column. Table 15 adds measured low-latency and uncapped profiles. Table 17 separates runtime-budget, topology, temporal-stability, and visual-richness subscores, so the low-latency profile is treated as a speed/clarity operating point rather than a visually identical replacement for full-visual rendering. Table 18 separates glyph-identity stability from screen-connectivity topology metrics; the real-orbit row has stable glyph identity while the screen-connectivity metric flags the sampled/open-curve structure. Table 19 unifies the runtime budget ladder through the 1200 FPS frame-build budget, Table 21 reports a longer 1200 FPS confidence run, Table 22 records adaptive low-latency activation under overload, and Table 20 defines the manual terminal-I/O capture protocol. The scope is explicitly `release-frame-build-no-terminal-io`. On the measured machine, the full-visual frame build fits a 60 FPS budget but not a 120 FPS budget; the medium-latency profile keeps afterimage history and fits a 240 FPS frame-build budget; the low-latency topology-dp profile skips high-cost background particles, afterimage, animated logo material, temporal color AA, and smoothing while retaining topology-dp orbit reconstruction. If its measured p99 row remains under 833.33 us/frame, the defensible high-FPS claim is 1200 FPS frame-build budget support. The uncapped row means the renderer does not intentionally sleep between frames, not that terminal display refresh is unlimited. The table does not measure terminal flush latency and should not be used to claim end-to-end terminal I/O performance.

## 7. Artifact

The artifact provides deterministic CSV and figure generation through:

```sh
bash scripts/reproduce_paper_artifacts.sh
```

Generated outputs include the topology metrics, stress metrics, speed-boundary table, translation-control table, stress interpretation table, sensitivity tables, runtime tables, figures, and an artifact manifest with SHA256 hashes. Terminal compatibility rows are artifact support only. They are not part of the core novelty claim.

## 8. Limitations

The method is specialized to moving line primitives represented by a discrete terminal glyph alphabet. It does not solve arbitrary text animation, terminal image conversion, pixel antialiasing, or vector rendering.

The strongest limitation is correspondence loss under extreme speed. The current method detects this boundary but does not recover stable identity when the candidate set no longer contains the true correspondence. The high-speed stress row should remain visible in the paper.

Glyph robustness is currently supported by controlled Unicode, ASCII, and degraded proxy alphabets. A real multi-font or multi-terminal capture row would strengthen the submission, but the current artifact must not claim font-independent proof.

Runtime evidence is scoped to release frame construction without terminal I/O. The current integrated artifact supports a 60 FPS full-visual frame-build claim, a 240 FPS medium-latency frame-build claim, and a 1200 FPS low-latency topology-dp frame-build budget claim. Terminal flush timing, terminal emulator behavior, and full end-to-end latency remain outside the measured claim.

Finally, the novelty claim is public-search hardened but should still use "to our knowledge" unless formal indexed database exports are completed.

Runtime measurements are single-machine artifact evidence unless Table 19 and
Table 21 are repeated on another CPU/OS environment. The manuscript should not
generalize the 60/240/1200 FPS frame-build tiers beyond measured rows.

## 9. Conclusion

This work frames moving terminal line glyphs as a topology-constrained temporal reconstruction problem and implements `topology-dp`, a metadata-evaluated path-topology assignment method. The method eliminates canonical corner and glyph identity flips under recoverable path correspondence, exposes stress and high-speed failure boundaries, and reports scoped runtime and sensitivity evidence. The appropriate claim is bounded but strong: topology-constrained temporal glyph reconstruction for discrete character-cell line primitives, with explicit correspondence-loss detection outside its recoverable regime.
