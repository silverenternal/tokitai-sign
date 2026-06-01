#!/usr/bin/env python3
import csv
import hashlib
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "artifact_manifest.csv"

ARTIFACTS = [
    "ARTIFACT.md",
    "README.md",
    "docs/paper-draft.md",
    "docs/cover-letter.md",
    "docs/novelty-review.md",
    "docs/paper-roadmap.md",
    "docs/target-journals.md",
    "docs/compatibility-capture.md",
    "docs/glyph-robustness.md",
    "docs/prior-art-export-log.md",
    "docs/prior-art-matrix.md",
    "docs/runtime-scope.md",
    "docs/claim-audit.md",
    "docs/submission-package.md",
    "docs/formal-problem.md",
    "docs/algorithm.md",
    "docs/baselines.md",
    "table_1_ablation_scores.csv",
    "table_2_dirty_budget_curves.csv",
    "table_3_terminal_matrix.csv",
    "table_4_glyph_reconstruction.csv",
    "table_5_dirty_decision_audit.csv",
    "table_6_topology_metrics.csv",
    "table_7_stress_topology_metrics.csv",
    "table_8_weight_sensitivity.csv",
    "table_9_runtime_complexity.csv",
    "table_10_integrated_runtime.csv",
    "table_11_speed_boundary.csv",
    "table_12_stress_interpretation.csv",
    "table_13_weight_stability_regions.csv",
    "table_14_translation_identity_control.csv",
    "table_15_runtime_degraded_profile.csv",
    "table_16_weight_grid.csv",
    "table_17_low_latency_quality_delta.csv",
    "table_18_low_latency_topology_metrics.csv",
    "table_19_runtime_budget_ladder.csv",
    "table_20_terminal_io_protocol.csv",
    "table_21_runtime_confidence_1200fps.csv",
    "table_22_adaptive_low_latency_policy.csv",
    "table_24_baseline_positioning.csv",
    "metric_validation_table.csv",
    "figure_1_method.csv",
    "figure_1_method.png",
    "figure_2_quality_budget_curve.png",
    "figure_3_metric_validation.png",
    "figure_4_glyph_reconstruction.png",
    "figure_5_topology_metrics.png",
    "figure_6_stress_degradation.png",
    "figure_7_weight_sensitivity.png",
    "figure_8_runtime_scaling.png",
    "figure_9_integrated_runtime.png",
    "figure_10_speed_boundary.png",
    "figure_11_weight_grid.png",
    "figure_12_low_latency_quality_delta.png",
    "figure_13_runtime_budget_ladder.png",
    "figure_14_runtime_confidence_1200fps.png",
    "figure_15_adaptive_low_latency_policy.png",
    "scripts/generate_paper_figures.py",
    "scripts/make_artifact_manifest.py",
    "scripts/reproduce_paper_artifacts.sh",
]


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def main() -> None:
    rows = []
    for relative in ARTIFACTS:
        path = ROOT / relative
        if not path.exists():
            rows.append(
                {
                    "path": relative,
                    "bytes": "",
                    "sha256": "",
                    "status": "missing",
                }
            )
            continue
        rows.append(
            {
                "path": relative,
                "bytes": path.stat().st_size,
                "sha256": sha256(path),
                "status": "present",
            }
        )

    with OUTPUT.open("w", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=["path", "bytes", "sha256", "status"])
        writer.writeheader()
        writer.writerows(rows)


if __name__ == "__main__":
    main()
