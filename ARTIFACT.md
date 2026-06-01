# Artifact Reproduction

This artifact reproduces the paper-facing tables and figures for
`tokitai-sign`.

## Environment

- Rust toolchain with Cargo.
- Python 3 with `matplotlib`.
- A terminal capable of running the binary for optional manual captures.

## One-Command Reproduction

```sh
bash scripts/reproduce_paper_artifacts.sh
```

The script runs:

- `cargo fmt -- --check`
- `cargo test`
- `cargo clippy -- -D warnings`
- all paper experiment CSV generators
- `python scripts/generate_paper_figures.py`
- `python scripts/make_artifact_manifest.py`

## Generated Tables

- `table_1_ablation_scores.csv`
- `table_2_dirty_budget_curves.csv`
- `table_3_terminal_matrix.csv`
- `table_4_glyph_reconstruction.csv`
- `table_5_dirty_decision_audit.csv`
- `table_6_topology_metrics.csv`
- `table_7_stress_topology_metrics.csv`
- `table_8_weight_sensitivity.csv`
- `table_9_runtime_complexity.csv`
- `table_10_integrated_runtime.csv`
- `table_11_speed_boundary.csv`
- `table_12_stress_interpretation.csv`
- `table_13_weight_stability_regions.csv`
- `table_14_translation_identity_control.csv`
- `table_15_runtime_degraded_profile.csv`
- `table_16_weight_grid.csv`
- `table_17_low_latency_quality_delta.csv`
- `table_18_low_latency_topology_metrics.csv`
- `table_19_runtime_budget_ladder.csv`
- `table_20_terminal_io_protocol.csv`
- `table_21_runtime_confidence_1200fps.csv`
- `table_22_adaptive_low_latency_policy.csv`
- `table_24_baseline_positioning.csv`
- `metric_validation_table.csv`

`table_7_stress_topology_metrics.csv` includes metadata-derived
`path_order_violation_rate`, `correspondence_lost_rate`, and
`stress_degradation_index`. The stress index is the max of corner instability,
stroke difference, path-order violation, and correspondence loss; raw columns
remain the primary evidence. `table_8` includes raw and normalized topology-dp
weight variants plus Unicode, ASCII, and degraded glyph-alphabet proxy rows.
`table_10` reports release frame-build timing without terminal I/O.
`table_11` sweeps inter-frame displacement to show the correspondence-loss
boundary using fixture-level recoverability labels, path-order,
screen-discontinuity, and correspondence-loss columns rather than
screen-coordinate corner-flip metrics. `table_12` gives a raw-metric-first
interpretation of each stress fixture. `table_13` summarizes tested weight
intervals around the defaults. `table_14` reports path-keyed identity stability
for translated low-speed motion. `table_15` reports measured full-visual,
low-latency, and uncapped frame-build rows. `table_16` provides a denser
five-point weight grid. `table_17` separates runtime-budget, topology,
temporal-stability, and visual-richness subscores for full, medium, and
low-latency profiles. `table_18` separates glyph-identity verdicts from
screen-connectivity topology verdicts. `table_19` reports 60, 120, 240, 1000,
and 1200 FPS frame-build budget percentages. `table_20` defines the manual
terminal-I/O protocol. `table_21` reports long-run 1200 FPS frame-build
confidence statistics, and `table_22` records the adaptive low-latency policy
scenario. `table_24` records reviewer-facing baseline positioning and explains
which systems are direct baselines versus adjacent related work. Terminal I/O
and display refresh remain out of scope for automated runtime claims.

## Generated Figures

- `figure_1_method.png`
- `figure_2_quality_budget_curve.png`
- `figure_3_metric_validation.png`
- `figure_4_glyph_reconstruction.png`
- `figure_5_topology_metrics.png`
- `figure_6_stress_degradation.png`
- `figure_7_weight_sensitivity.png`
- `figure_8_runtime_scaling.png`
- `figure_9_integrated_runtime.png`
- `figure_10_speed_boundary.png`
- `figure_11_weight_grid.png`
- `figure_12_low_latency_quality_delta.png`
- `figure_13_runtime_budget_ladder.png`
- `figure_14_runtime_confidence_1200fps.png`
- `figure_15_adaptive_low_latency_policy.png`

## Artifact Manifest

- `artifact_manifest.csv`

The manifest records the path, byte size, SHA256 hash, and present/missing
status for the manuscript notes, generated CSVs, generated figures, and
reproduction scripts referenced by the paper package.

## Manual Terminal Capture

`table_3_terminal_matrix.csv` includes the current environment and template
rows. Real cross-terminal claims require manually running the capture workflow
in `docs/compatibility-capture.md` on at least three terminal emulators using
the same font, size, profile, and motion preset.

This manual step is intentionally not marked as reproduced by the one-command
script. The paper must keep cross-terminal claims limited to measured rows until
the template rows are replaced with real captures.

Terminal I/O timing is intentionally excluded from `table_10`; the table
measures release frame construction and topology overhead only.

`docs/glyph-robustness.md` records the current boundary between synthetic
glyph-alphabet proxy rows and real terminal/font captures. It includes one
documented local terminal-layer row, but broader font-independent or
cross-terminal claims still require visual captures on named terminals and
fonts.

`docs/runtime-scope.md` records the full-visual frame-build cost, low-latency
1200 FPS frame-build budget evidence, uncapped mode semantics, and terminal I/O
exclusion. `docs/prior-art-export-log.md` records
the formal indexed-database export gate without fabricating authenticated hit
counts. `docs/claim-audit.md` records wording that must remain bounded before
submission.
