# Claim Audit

Date: 2026-06-01.

Audited files:

- `README.md`
- `docs/paper-draft.md`
- `ARTIFACT.md`
- `docs/formal-problem.md`
- `docs/glyph-robustness.md`
- `docs/prior-art-matrix.md`
- `docs/runtime-scope.md`

## Required Claim Boundaries

- First-known wording must use "To our knowledge".
- The core novelty is topology-constrained temporal reconstruction for moving
  line primitives represented by a discrete character-cell glyph alphabet.
- High-speed behavior is correspondence-loss detection, not recovery.
- Glyph robustness is proxy-based unless a measured terminal/font row is
  explicitly named.
- Runtime claims are release frame-build claims unless terminal I/O timing is
  measured separately.
- 1200 FPS frame-build claims are limited to the measured low-latency
  topology-dp profile on the measured machine and only pass if the p99 row
  stays below 833.33 us/frame.
- Medium-latency should be described as a quality ladder profile, not as a
  1200 FPS profile.
- Low-latency may win runtime-budget quality; do not describe it as globally
  visually superior to full-visual rendering.
- `--uncapped` means no intentional sleep; it is not a visible-refresh claim.
- `stress_degradation_index` is a plotting diagnostic; raw Table 7 metrics are
  primary evidence.
- Baseline comparisons must be phrased as fair design-point comparisons, not as
  strawman failures.
- Table 18's `identity_verdict` supports glyph identity stability; its
  `topology_verdict` is a screen-connectivity diagnostic and must not be
  silently ignored.
- Runtime tiers are single-machine artifact evidence unless cross-machine rows
  are added.

## Current Audit Result

The manuscript draft keeps the claim bounded and avoids universal terminal
flicker-elimination wording. The artifact docs now state that terminal I/O is
excluded, that proxy glyph alphabets are not a multi-font proof, that runtime
tiers are measured-machine evidence, and that formal indexed prior-art exports
remain a manual pre-submission gate.

## Phrases To Avoid

- "first temporal antialiasing method for terminals"
- "font-independent"
- "universal flicker elimination"
- "guaranteed stable under arbitrary motion"
- "1200 FPS terminal performance" or "unlimited FPS"
- "end-to-end latency" without terminal I/O measurement
