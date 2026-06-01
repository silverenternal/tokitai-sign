# Submission Package

Date: 2026-06-01.

## Target Venues

Primary target: Multimedia Tools and Applications.

Rationale: the work is a reproducible multimedia/graphics tool paper with a
narrow method contribution, deterministic artifact, runtime tables, and
submission-ready reproduction script.

Backup target: The Visual Computer.

Rationale: the graphics-method framing is relevant, but the paper would need
stronger visual captures, polished figures, and more formal related-work
positioning to compete as a computer-graphics method submission.

Final quartile/indexing status must be rechecked immediately before submission.

## Bounded Contribution Bullets

1. A topology-constrained temporal reconstruction formulation for moving line
   primitives represented by a discrete character-cell glyph alphabet.
2. `topology-dp`, a local fixed-candidate assignment method that scores
   temporal identity, local path distance, neighbor topology, and corner
   identity.
3. Metadata-backed metrics and stress fixtures that separate recoverable
   correspondence, path-order corruption, topology mutation, crossing
   ambiguity, and correspondence loss.
4. A reproducible Rust artifact with one-command generation for tables,
   figures, and an artifact manifest.

## Novelty Summary For Cover Letter

This submission studies a narrow but under-addressed reconstruction problem in
terminal character-cell graphics: moving line primitives must be represented by
topology-sensitive glyph sequences after projection to a discrete terminal grid.
The paper proposes topology-constrained temporal glyph reconstruction and
evaluates it with metadata-backed path identity metrics, stress-boundary
fixtures, parameter sensitivity, and scoped runtime measurements. The claim is
bounded: the method preserves symbolic line-glyph identity when correspondence
is recoverable and explicitly detects correspondence loss outside that regime.

The full editable cover letter draft is maintained in
`docs/cover-letter.md`. Keep its runtime wording scoped to "1200 FPS
frame-build budget" and do not shorten it to "1200 FPS terminal performance".

## Figure Captions

- Figure 1: Paper artifact pipeline from continuous line primitive to
  path-indexed glyph assignment, dirty rendering, and metric output.
- Figure 2: Visual quality and dirty-cell pressure under dirty-budget
  constraints.
- Figure 3: Metric validation on golden and intentionally degraded fixtures.
- Figure 4: Glyph reconstruction ablation on real-orbit and glyph-stress
  fixtures.
- Figure 5: Topology metric comparison for canonical reconstruction fixtures.
- Figure 6: Stress degradation diagnostic; Table 7 raw metrics remain primary.
- Figure 7: Weight sensitivity over raw and normalized topology-dp variants.
- Figure 8: Fixed-candidate topology assignment microbenchmark.
- Figure 9: Integrated release frame-build timing without terminal I/O.
- Figure 10: Speed-boundary sweep with fixture-level recoverability labels and
  method-level correspondence-loss detection.
- Figure 11: Dense five-point weight grid around the default weights.
- Figure 12: Low-latency quality/runtime delta against the 1200 FPS
  frame-build budget.
- Figure 13: Unified frame-build budget ladder for full-visual, low-latency,
  uncapped, and fallback profiles.
- Figure 14: Long-run low-latency 1200 FPS frame-build confidence statistics.
- Figure 15: Adaptive low-latency activation under synthetic overload.

## Required Manual Attachments

- Authenticated ACM DL, IEEE Xplore, Scopus, Web of Science, and SpringerLink
  export records.
- At least one visual terminal/font screenshot with box-drawing connectivity
  certified. The caption must name terminal emulator, font, font size,
  resolution or terminal grid size, command line, date, and git commit.
- Optional qualitative video or slow-motion capture may be attached as visual
  evidence, but it must not be treated as the primary quantitative FPS proof.
- Target journal reference style conversion.
- Final title page, author metadata, data availability, conflict-of-interest,
  and funding statements.

## Claim Guardrails

- Use "To our knowledge" for first-known claims.
- Do not claim universal terminal flicker elimination.
- Do not claim font-independent behavior from proxy glyph rows.
- Claim 1200 FPS frame-build budget support only for the measured low-latency
  topology-dp profile if p99 stays below 833.33 us/frame.
- Treat medium-latency as a 240 FPS frame-build profile unless stronger
  measured rows support a higher claim.
- Do not claim terminal I/O latency without a separate terminal flush capture.
