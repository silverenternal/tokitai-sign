#!/usr/bin/env python3
import csv
from pathlib import Path

import matplotlib.pyplot as plt


ROOT = Path(__file__).resolve().parents[1]


def read_csv(path):
    with (ROOT / path).open(newline="") as handle:
        return list(csv.DictReader(handle))


def figure_method():
    rows = read_csv("figure_1_method.csv")
    labels = [row["stage"].replace("_", "\n") for row in rows]
    notes = [row["output"].replace("_", " ") for row in rows]

    fig, ax = plt.subplots(figsize=(9.5, 2.8))
    ax.axis("off")
    y = 0.55
    for index, (label, note) in enumerate(zip(labels, notes)):
        x = 0.08 + index * 0.23
        ax.text(
            x,
            y,
            label,
            ha="center",
            va="center",
            fontsize=10,
            bbox={"boxstyle": "round,pad=0.35", "fc": "#eef6ff", "ec": "#2d6ea3"},
        )
        ax.text(x, y - 0.23, note, ha="center", va="center", fontsize=8, color="#38434f")
        if index < len(labels) - 1:
            ax.annotate(
                "",
                xy=(x + 0.14, y),
                xytext=(x + 0.09, y),
                arrowprops={"arrowstyle": "->", "lw": 1.6, "color": "#2d6ea3"},
            )
    ax.set_title("Perceptual Dirty Rendering Pipeline", fontsize=13, weight="bold")
    fig.tight_layout()
    fig.savefig(ROOT / "figure_1_method.png", dpi=220)
    plt.close(fig)


def figure_budget_curve():
    rows = read_csv("table_2_dirty_budget_curves.csv")
    modes = ["naive", "uniform-threshold", "role-aware", "full"]
    colors = {
        "naive": "#8c564b",
        "uniform-threshold": "#1f77b4",
        "role-aware": "#2ca02c",
        "full": "#111111",
    }

    fig, (ax_score, ax_pressure) = plt.subplots(1, 2, figsize=(10.5, 4.0))
    for mode in modes:
        series = [row for row in rows if row["dirty_mode"] == mode]
        budgets = [int(row["budget"]) for row in series]
        scores = [float(row["visual_quality_score"]) for row in series]
        pressure = [float(row["dirty_cell_pressure"]) for row in series]
        ax_score.plot(budgets, scores, marker="o", label=mode, color=colors[mode])
        ax_pressure.plot(budgets, pressure, marker="o", label=mode, color=colors[mode])

    ax_score.set_title("Quality vs Dirty-Cell Budget")
    ax_score.set_xlabel("Dirty-cell budget per frame")
    ax_score.set_ylabel("Visual quality score")
    ax_score.grid(True, alpha=0.25)
    ax_score.legend(fontsize=8)

    ax_pressure.set_title("Dirty Pressure vs Budget")
    ax_pressure.set_xlabel("Dirty-cell budget per frame")
    ax_pressure.set_ylabel("Dirty cell pressure")
    ax_pressure.axhline(1.0, color="#cc3333", lw=1.2, ls="--")
    ax_pressure.grid(True, alpha=0.25)

    fig.tight_layout()
    fig.savefig(ROOT / "figure_2_quality_budget_curve.png", dpi=220)
    plt.close(fig)


def figure_glyph_and_metrics():
    ablations = read_csv("table_1_ablation_scores.csv")
    glyph_rows = [row for row in ablations if row["experiment"] == "glyph-stress"]
    metrics = read_csv("metric_validation_table.csv")
    metric_rows = [row for row in metrics if row["expected_failure"] != "reference"]

    fig, (ax_glyph, ax_metric) = plt.subplots(1, 2, figsize=(11.0, 4.0))
    glyph_labels = [row["glyph_history"] for row in glyph_rows]
    glyph_flips = [float(row["glyph_flips_per_second"]) for row in glyph_rows]
    ax_glyph.bar(glyph_labels, glyph_flips, color=["#a04b42", "#4472a8", "#2f8f4e"])
    ax_glyph.set_title("Glyph History Ablation")
    ax_glyph.set_ylabel("Glyph flips per second")
    ax_glyph.grid(axis="y", alpha=0.25)

    metric_labels = [
        row["expected_failure"].split()[0].replace("/", "\n") for row in metric_rows
    ]
    metric_scores = [float(row["visual_quality_score"]) for row in metric_rows]
    reference = float(metrics[0]["visual_quality_score"])
    ax_metric.bar(metric_labels, metric_scores, color="#6678a9")
    ax_metric.axhline(reference, color="#111111", lw=1.2, ls="--", label="golden")
    ax_metric.set_title("Controlled Metric Validation")
    ax_metric.set_ylabel("Visual quality score")
    ax_metric.tick_params(axis="x", labelrotation=20)
    ax_metric.legend(fontsize=8)
    ax_metric.grid(axis="y", alpha=0.25)

    fig.tight_layout()
    fig.savefig(ROOT / "figure_3_metric_validation.png", dpi=220)
    plt.close(fig)


def figure_glyph_reconstruction():
    path = ROOT / "table_4_glyph_reconstruction.csv"
    if not path.exists():
        return
    rows = read_csv("table_4_glyph_reconstruction.csv")
    datasets = ["real-orbit", "glyph-stress"]
    modes = ["off", "screen-cell", "path"]
    colors = {"off": "#a04b42", "screen-cell": "#4472a8", "path": "#2f8f4e"}

    fig, axes = plt.subplots(1, 2, figsize=(10.5, 4.0), sharey=False)
    for ax, dataset in zip(axes, datasets):
        subset = [row for row in rows if row["experiment"] == dataset]
        values = {
            row["glyph_history"]: float(row["glyph_flips_per_second"])
            for row in subset
        }
        ax.bar(modes, [values.get(mode, 0.0) for mode in modes], color=[colors[m] for m in modes])
        ax.set_title(dataset.replace("-", " ").title())
        ax.set_ylabel("Glyph flips per second")
        ax.grid(axis="y", alpha=0.25)

    fig.tight_layout()
    fig.savefig(ROOT / "figure_4_glyph_reconstruction.png", dpi=220)
    plt.close(fig)


def figure_topology_metrics():
    path = ROOT / "table_6_topology_metrics.csv"
    if not path.exists():
        return
    rows = read_csv("table_6_topology_metrics.csv")
    datasets = ["real-orbit", "glyph-stress"]
    modes = ["off", "screen-cell", "path", "topology-dp"]
    colors = {
        "off": "#a04b42",
        "screen-cell": "#4472a8",
        "path": "#8a6f2a",
        "topology-dp": "#2f8f4e",
    }

    fig, axes = plt.subplots(1, 2, figsize=(11.0, 4.0), sharey=True)
    for ax, dataset in zip(axes, datasets):
        subset = [row for row in rows if row["dataset"] == dataset]
        values = {
            row["glyph_history"]: float(row["corner_glyph_instability"])
            for row in subset
        }
        ax.bar(modes, [values.get(mode, 0.0) for mode in modes], color=[colors[m] for m in modes])
        ax.set_title(dataset.replace("-", " ").title())
        ax.set_ylabel("Corner identity flip rate")
        ax.tick_params(axis="x", labelrotation=15)
        ax.grid(axis="y", alpha=0.25)

    fig.tight_layout()
    fig.savefig(ROOT / "figure_5_topology_metrics.png", dpi=220)
    plt.close(fig)


def figure_stress_degradation():
    path = ROOT / "table_7_stress_topology_metrics.csv"
    if not path.exists():
        return
    rows = read_csv("table_7_stress_topology_metrics.csv")
    datasets = [
        "canonical",
        "shuffled-path",
        "high-speed",
        "topology-break",
        "line-crossing",
        "bounded-jitter",
        "shape-continuity",
    ]
    modes = ["path", "topology-dp"]
    colors = {"path": "#8a6f2a", "topology-dp": "#2f8f4e"}

    fig, ax = plt.subplots(figsize=(10.8, 4.2))
    width = 0.36
    x = list(range(len(datasets)))
    for offset, mode in enumerate(modes):
        values = []
        for dataset in datasets:
            row = next(
                row for row in rows if row["dataset"] == dataset and row["glyph_history"] == mode
            )
            values.append(float(row["stress_degradation_index"]))
        positions = [value + (offset - 0.5) * width for value in x]
        ax.bar(positions, values, width=width, label=mode, color=colors[mode])
    ax.set_xticks(x)
    ax.set_xticklabels([dataset.replace("-", "\n") for dataset in datasets])
    ax.set_ylabel("Stress degradation index")
    ax.set_title("Stress Fixture Degradation")
    ax.legend()
    ax.grid(axis="y", alpha=0.25)
    fig.tight_layout()
    fig.savefig(ROOT / "figure_6_stress_degradation.png", dpi=220)
    plt.close(fig)


def figure_weight_sensitivity():
    path = ROOT / "table_8_weight_sensitivity.csv"
    if not path.exists():
        return
    rows = read_csv("table_8_weight_sensitivity.csv")
    datasets = [
        "real-orbit",
        "glyph-stress",
        "glyph-stress-ascii",
        "glyph-stress-degraded",
        "high-speed",
        "bounded-jitter",
    ]

    fig, ax = plt.subplots(figsize=(10.2, 4.0))
    variants = [
        f'{row["variant"]}\n{row.get("normalization", "raw")}'
        for row in rows
        if row["dataset"] == "real-orbit"
    ]
    x = list(range(len(variants)))
    for dataset in datasets:
        series = [row for row in rows if row["dataset"] == dataset]
        flips = [float(row["corner_identity_flip_rate"]) for row in series]
        ax.plot(x, flips, marker="o", label=dataset)
    ax.set_xticks(x)
    ax.set_xticklabels([variant.replace("-", "\n") for variant in variants], fontsize=8)
    ax.set_xlabel("Weight perturbation")
    ax.set_ylabel("Corner identity flip rate")
    ax.set_title("Topology-DP Weight Sensitivity")
    ax.grid(True, alpha=0.25)
    ax.legend(fontsize=8)
    fig.tight_layout()
    fig.savefig(ROOT / "figure_7_weight_sensitivity.png", dpi=220)
    plt.close(fig)


def figure_runtime_scaling():
    path = ROOT / "table_9_runtime_complexity.csv"
    if not path.exists():
        return
    rows = read_csv("table_9_runtime_complexity.csv")
    samples = [int(row["path_samples"]) for row in rows]
    mean = [float(row["mean_us"]) for row in rows]
    p99 = [float(row["p99_us"]) for row in rows]

    fig, ax = plt.subplots(figsize=(9.5, 4.0))
    ax.plot(samples, mean, marker="o", label="mean")
    ax.plot(samples, p99, marker="o", label="p99")
    ax.axhline(8333.33, color="#cc3333", lw=1.1, ls="--", label="120 FPS budget")
    ax.axhline(16666.67, color="#444444", lw=1.1, ls=":", label="60 FPS budget")
    ax.set_xlabel("Path samples")
    ax.set_ylabel("Assignment time (us)")
    ax.set_title("Topology-DP Runtime Scaling")
    ax.grid(True, alpha=0.25)
    ax.legend(fontsize=8)
    fig.tight_layout()
    fig.savefig(ROOT / "figure_8_runtime_scaling.png", dpi=220)
    plt.close(fig)


def figure_integrated_runtime():
    path = ROOT / "table_10_integrated_runtime.csv"
    if not path.exists():
        return
    rows = read_csv("table_10_integrated_runtime.csv")
    modes = [row["glyph_history"] for row in rows]
    mean_key = "mean_build_time_us" if "mean_build_time_us" in rows[0] else "mean_frame_us"
    p99_key = "p99_build_time_us" if "p99_build_time_us" in rows[0] else "p99_frame_us"
    mean = [float(row[mean_key]) for row in rows]
    p99 = [float(row[p99_key]) for row in rows]

    fig, ax = plt.subplots(figsize=(9.5, 4.0))
    x = list(range(len(modes)))
    width = 0.36
    ax.bar([value - width / 2 for value in x], mean, width=width, label="mean")
    ax.bar([value + width / 2 for value in x], p99, width=width, label="p99")
    ax.axhline(8333.33, color="#cc3333", lw=1.1, ls="--", label="120 FPS budget")
    ax.set_xticks(x)
    ax.set_xticklabels(modes)
    ax.set_ylabel("Frame build time (us)")
    ax.set_title("Integrated Frame Runtime")
    ax.grid(axis="y", alpha=0.25)
    ax.legend(fontsize=8)
    fig.tight_layout()
    fig.savefig(ROOT / "figure_9_integrated_runtime.png", dpi=220)
    plt.close(fig)


def figure_speed_boundary():
    path = ROOT / "table_11_speed_boundary.csv"
    if not path.exists():
        return
    rows = read_csv("table_11_speed_boundary.csv")
    modes = ["path", "topology-dp"]
    colors = {"path": "#8a6f2a", "topology-dp": "#2f8f4e"}

    fig, ax = plt.subplots(figsize=(9.5, 4.0))
    for mode in modes:
        series = [row for row in rows if row["glyph_history"] == mode]
        speeds = [int(row["cells_per_frame"]) for row in series]
        loss = [float(row["correspondence_lost_rate"]) for row in series]
        discontinuity = [float(row["screen_discontinuity_rate"]) for row in series]
        ax.plot(
            speeds,
            discontinuity,
            marker="o",
            label=f"{mode} screen discontinuity",
            color=colors[mode],
        )
        if mode == "topology-dp":
            ax.plot(
                speeds,
                loss,
                marker="s",
                ls="--",
                label="topology-dp correspondence loss",
                color="#cc3333",
            )
    ax.set_xlabel("Inter-frame displacement (cells/frame)")
    ax.set_ylabel("Rate")
    ax.set_title("Speed Boundary and Correspondence Loss")
    ax.grid(True, alpha=0.25)
    ax.legend(fontsize=8)
    fig.tight_layout()
    fig.savefig(ROOT / "figure_10_speed_boundary.png", dpi=220)
    plt.close(fig)


def figure_weight_grid():
    path = ROOT / "table_16_weight_grid.csv"
    if not path.exists():
        return
    rows = read_csv("table_16_weight_grid.csv")
    axes = ["temporal", "local", "topology", "corner"]

    fig, ax = plt.subplots(figsize=(10.0, 4.0))
    for axis in axes:
        series = [
            row
            for row in rows
            if row["weight_axis"] == axis and row["dataset"] == "glyph-stress"
        ]
        multipliers = [float(row["multiplier"]) for row in series]
        flips = [float(row["corner_identity_flip_rate"]) for row in series]
        ax.plot(multipliers, flips, marker="o", label=axis)
    ax.axvline(1.0, color="#222222", lw=1.0, ls=":", label="default")
    ax.set_xlabel("Single-axis multiplier")
    ax.set_ylabel("Corner identity flip rate")
    ax.set_title("Dense Weight Grid Around Defaults")
    ax.grid(True, alpha=0.25)
    ax.legend(fontsize=8)
    fig.tight_layout()
    fig.savefig(ROOT / "figure_11_weight_grid.png", dpi=220)
    plt.close(fig)


def figure_low_latency_quality_delta():
    path = ROOT / "table_17_low_latency_quality_delta.csv"
    if not path.exists():
        return
    rows = read_csv("table_17_low_latency_quality_delta.csv")
    profiles = [row["profile"].replace("-topology-dp", "").replace("-", "\n") for row in rows]
    build = [float(row["mean_build_time_us"]) for row in rows]
    runtime_quality = [float(row["runtime_budget_quality"]) for row in rows]
    richness = [float(row["visual_richness"]) for row in rows]
    budget = 833.33

    fig, (ax_time, ax_quality) = plt.subplots(1, 2, figsize=(10.4, 4.0))
    ax_time.bar(profiles, build, color=["#444444", "#8a6f2a", "#2f8f4e"])
    ax_time.axhline(budget, color="#cc3333", lw=1.1, ls="--", label="1200 FPS frame-build budget")
    ax_time.set_ylabel("Mean build time (us)")
    ax_time.set_title("Low-Latency Runtime")
    ax_time.grid(axis="y", alpha=0.25)
    ax_time.legend(fontsize=8)

    x = list(range(len(rows)))
    width = 0.36
    ax_quality.bar([value - width / 2 for value in x], runtime_quality, width=width, label="runtime")
    ax_quality.bar([value + width / 2 for value in x], richness, width=width, label="richness")
    ax_quality.set_xticks(x)
    ax_quality.set_xticklabels(profiles)
    ax_quality.set_ylabel("Subscore")
    ax_quality.set_title("Runtime vs Visual Richness")
    ax_quality.grid(axis="y", alpha=0.25)
    ax_quality.legend(fontsize=8)
    fig.tight_layout()
    fig.savefig(ROOT / "figure_12_low_latency_quality_delta.png", dpi=220)
    plt.close(fig)


def figure_runtime_budget_ladder():
    path = ROOT / "table_19_runtime_budget_ladder.csv"
    if not path.exists():
        return
    rows = read_csv("table_19_runtime_budget_ladder.csv")
    profiles = [row["profile"].replace("-", "\n") for row in rows]
    mean = [float(row["mean_build_time_us"]) for row in rows]
    p99 = [float(row["p99_build_time_us"]) for row in rows]

    fig, ax = plt.subplots(figsize=(10.8, 4.3))
    x = list(range(len(rows)))
    width = 0.36
    ax.bar([value - width / 2 for value in x], mean, width=width, label="mean", color="#4472a8")
    ax.bar([value + width / 2 for value in x], p99, width=width, label="p99", color="#8a6f2a")
    for label, budget, color, style in [
        ("120 FPS", 8333.33, "#666666", ":"),
        ("1000 FPS", 1000.0, "#cc7a33", "--"),
        ("1200 FPS", 833.33, "#cc3333", "-."),
    ]:
        ax.axhline(budget, color=color, lw=1.1, ls=style, label=f"{label} budget")
    ax.set_xticks(x)
    ax.set_xticklabels(profiles, fontsize=8)
    ax.set_ylabel("Frame build time (us)")
    ax.set_title("Frame-Build Budget Ladder")
    ax.grid(axis="y", alpha=0.25)
    ax.legend(fontsize=8)
    fig.tight_layout()
    fig.savefig(ROOT / "figure_13_runtime_budget_ladder.png", dpi=220)
    plt.close(fig)


def figure_runtime_confidence_1200fps():
    path = ROOT / "table_21_runtime_confidence_1200fps.csv"
    if not path.exists():
        return
    row = read_csv("table_21_runtime_confidence_1200fps.csv")[0]
    labels = ["mean", "median", "p95", "p99", "p999", "worst"]
    values = [float(row[f"{label}_us"]) for label in labels]
    fig, ax = plt.subplots(figsize=(9.2, 4.0))
    ax.bar(labels, values, color="#4472a8")
    ax.axhline(833.33, color="#cc3333", lw=1.1, ls="--", label="1200 FPS budget")
    ax.set_ylabel("Frame build time (us)")
    ax.set_title("Low-Latency 1200 FPS Confidence")
    ax.grid(axis="y", alpha=0.25)
    ax.legend(fontsize=8)
    fig.tight_layout()
    fig.savefig(ROOT / "figure_14_runtime_confidence_1200fps.png", dpi=220)
    plt.close(fig)


def figure_adaptive_low_latency_policy():
    path = ROOT / "table_22_adaptive_low_latency_policy.csv"
    if not path.exists():
        return
    rows = read_csv("table_22_adaptive_low_latency_policy.csv")
    frames = [int(row["frame"]) for row in rows]
    quality = [float(row["quality_percent"]) for row in rows]
    active = [100.0 if row["low_latency_active"] == "true" else 0.0 for row in rows]
    target = [float(row["target_fps"]) for row in rows]

    fig, ax = plt.subplots(figsize=(10.0, 4.0))
    ax.plot(frames, quality, label="quality percent", color="#4472a8")
    ax.plot(frames, active, label="low latency active", color="#2f8f4e", ls="--")
    ax.plot(frames, target, label="target fps", color="#8a6f2a", alpha=0.75)
    ax.set_xlabel("Synthetic frame")
    ax.set_ylabel("Policy state")
    ax.set_title("Adaptive Low-Latency Policy")
    ax.grid(True, alpha=0.25)
    ax.legend(fontsize=8)
    fig.tight_layout()
    fig.savefig(ROOT / "figure_15_adaptive_low_latency_policy.png", dpi=220)
    plt.close(fig)


def main():
    figure_method()
    figure_budget_curve()
    figure_glyph_and_metrics()
    figure_glyph_reconstruction()
    figure_topology_metrics()
    figure_stress_degradation()
    figure_weight_sensitivity()
    figure_runtime_scaling()
    figure_integrated_runtime()
    figure_speed_boundary()
    figure_weight_grid()
    figure_low_latency_quality_delta()
    figure_runtime_budget_ladder()
    figure_runtime_confidence_1200fps()
    figure_adaptive_low_latency_policy()


if __name__ == "__main__":
    main()
