#!/usr/bin/env bash
set -euo pipefail

cargo fmt -- --check
cargo test
cargo clippy -- -D warnings

cargo run --release -- --snapshot --profile ultra --motion prime --no-calibration >/tmp/tokitai_snapshot.json
cargo run --release -- --paper-experiment ablations --output table_1_ablation_scores.csv --no-calibration
cargo run --release -- --paper-experiment dirty-budget --output table_2_dirty_budget_curves.csv --no-calibration
cargo run --release -- --paper-experiment terminal-matrix --output table_3_terminal_matrix.csv --no-calibration
cargo run --release -- --paper-experiment metric-validation --output metric_validation_table.csv --no-calibration
cargo run --release -- --paper-experiment glyph-reconstruction --output table_4_glyph_reconstruction.csv --no-calibration
cargo run --release -- --paper-experiment dirty-audit --output table_5_dirty_decision_audit.csv --no-calibration
cargo run --release -- --paper-experiment topology-metrics --output table_6_topology_metrics.csv --no-calibration
cargo run --release -- --paper-experiment topology-stress --output table_7_stress_topology_metrics.csv --no-calibration
cargo run --release -- --paper-experiment weight-sensitivity --output table_8_weight_sensitivity.csv --no-calibration
cargo run --release -- --paper-experiment runtime-complexity --output table_9_runtime_complexity.csv --no-calibration
cargo run --release -- --paper-experiment integrated-runtime --output table_10_integrated_runtime.csv --no-calibration
cargo run --release -- --paper-experiment speed-boundary --output table_11_speed_boundary.csv --no-calibration
cargo run --release -- --paper-experiment stress-interpretation --output table_12_stress_interpretation.csv --no-calibration
cargo run --release -- --paper-experiment weight-stability --output table_13_weight_stability_regions.csv --no-calibration
cargo run --release -- --paper-experiment translation-control --output table_14_translation_identity_control.csv --no-calibration
cargo run --release -- --paper-experiment runtime-degraded --output table_15_runtime_degraded_profile.csv --no-calibration
cargo run --release -- --paper-experiment weight-grid --output table_16_weight_grid.csv --no-calibration
cargo run --release -- --paper-experiment low-latency-quality-delta --output table_17_low_latency_quality_delta.csv --no-calibration
cargo run --release -- --paper-experiment low-latency-topology-metrics --output table_18_low_latency_topology_metrics.csv --no-calibration
cargo run --release -- --paper-experiment runtime-budget-ladder --output table_19_runtime_budget_ladder.csv --no-calibration
cargo run --release -- --paper-experiment terminal-io-protocol --output table_20_terminal_io_protocol.csv --no-calibration
cargo run --release -- --paper-experiment runtime-confidence-1200fps --output table_21_runtime_confidence_1200fps.csv --no-calibration
cargo run --release -- --paper-experiment adaptive-low-latency-policy --output table_22_adaptive_low_latency_policy.csv --no-calibration

python scripts/generate_paper_figures.py
python scripts/make_artifact_manifest.py
