# tokitai-sign

`tokitai-sign` is a high-frame-rate terminal splash animation.

It starts with a short deep-blue loading bar, then keeps a persistent animated `tokitai` sign on screen until you exit. The scene combines a deep-blue/black palette, a differential cosmic particle field, a moving orbit line, adaptive dirty-cell rendering, and optional mouse-driven particle physics.

## Run

```sh
cargo run
```

Exit with `q`, `Esc`, or `Ctrl+C`.

## Options

```sh
cargo run -- --no-loader
cargo run -- --speed fast
cargo run -- --speed slow
cargo run -- --profile calm
cargo run -- --profile benchmark
cargo run -- --motion aurora
cargo run -- --motion pulse
cargo run -- --theme tokitai.theme
cargo run -- --record metrics.log
cargo run -- --replay metrics.log
cargo run -- --record-frames frames.jsonl
cargo run -- --score-frames frames.jsonl
cargo run -- --dirty-mode role-aware
cargo run -- --glyph-history path
cargo run -- --fixed-dirty-budget 320
cargo run -- --low-latency
cargo run -- --uncapped
cargo run -- --paper-experiment ablations --output table_1_ablation_scores.csv
cargo run -- --paper-experiment dirty-budget --output table_2_dirty_budget_curves.csv
cargo run -- --paper-experiment terminal-matrix --output table_3_terminal_matrix.csv
cargo run -- --paper-experiment metric-validation --output metric_validation_table.csv
cargo run -- --paper-experiment glyph-reconstruction --output table_4_glyph_reconstruction.csv
cargo run -- --paper-experiment dirty-audit --output table_5_dirty_decision_audit.csv
python scripts/generate_paper_figures.py
cargo run -- --snapshot
cargo run -- --inspect
cargo run -- --no-calibration
cargo run -- --help
cargo run -- --version
```

Supported speed values are `slow`, `normal`, and `fast`.
Supported profiles are `ultra`, `cinematic`, `calm`, and `benchmark`.
Supported motion presets are `prime`, `aurora`, and `pulse`.

Set `TOKITAI_BRAILLE=1` to use Braille orbit glyphs, or `TOKITAI_ASCII=1` for an ASCII-safe fallback.

## Visual System

The animation is split into independent visual layers:

| Layer | Behavior |
| --- | --- |
| Logo | Persistent `tokitai` wordmark with smooth OKLCH color motion and role-aware contrast. |
| Orbit line | A long partial line travels around the logo on a rectangular path without drawing a full box. |
| Cosmic background | Six sparse particle layers move with different speed, direction, shear, wave amplitude, and tail spacing. |
| Mouse trail | When left mouse input is available and held, particles are injected into a velocity grid and advected as a curl-preserving wake. |
| Afterimage | Motion layers use FPS-adapted temporal kernels so trails stay continuous without blurring text. |

The cosmic background is deliberately separate from the mouse trail. Background particles keep their own differential deep-space motion; mouse interaction only affects the foreground particle wake.

## Algorithms

The renderer uses terminal-friendly versions of several graphics techniques:

- A compact deterministic STBN mask volume for particle visibility, shimmer, tail dither, and rare event timing.
- Perceptual error fields to keep stochastic particles away from the logo and orbit focus area.
- A low-frequency cosmic density field that creates negative space and peripheral deep-space bands.
- Velocity-aligned anisotropic sampling for directional particle trails.
- Rare deterministic comet events that stay outside the logo focus area.
- A low-resolution vorticity-preserving velocity grid plus paired vortex particles for mouse-driven wake motion.
- Cell-space temporal anti-aliasing, afterimage, and particle tails driven by one FPS-adapted reconstruction profile.
- OKLCH constrained color optimization with terminal appearance profiles for gamma, black floor, glyph weight, and truecolor reliability.
- Role-aware visual scoring for foreground clarity, background atmosphere, flicker, clustering, contrast hierarchy, temporal spectrum, particle spectral quality, and stable foreground motion.
- Lightweight deterministic parameter search used by tests to compare tuned visual parameters against a baseline objective.

Inspect mode enables runtime controls:

| Key | Action |
| --- | --- |
| `m` | Cycle motion preset |
| `p` | Pause or resume |
| `b` | Switch to benchmark profile |
| `c` | Switch to calm profile |
| `u` | Switch to ultra profile |

## Theme

An optional theme file uses simple `key=value` lines:

```txt
logo_base=#3497e2
logo_shadow=#2c80c6
logo_highlight=#ccf4ff
trail_head=#daf8ff
trail_body=#5cc2f6
trail_tail=#246eb2
accent=#98dcff
dim=#4e84b8
contrast_floor=0.33
trail_span=34
rhythm_intensity=1.0
```

An example theme is available at `examples/tokitai.theme`.

## Rendering Notes

`tokitai-sign` uses adaptive frame pacing. Ultra and benchmark profiles start near 120 FPS, then step through 90, 75, and 60 FPS if the terminal misses repeated frame deadlines. Animation time stays based on elapsed seconds, so motion speed remains stable when the frame tier changes.

Use `--low-latency` for high-FPS frame-build experiments. It keeps the
topology-dp orbit reconstruction but skips high-cost background particles,
afterimage, animated logo material, temporal color AA, and smoothing. Use
`--uncapped` to disable active frame sleeping; this means the app does not
intentionally cap the build loop, not that terminal display refresh is
unlimited. Paper-facing 1200 FPS wording is scoped to measured frame-build
budget rows, not visible terminal refresh.

Benchmark mode displays FPS, selected tier, missed deadlines, worst frame time, dirty cells, ANSI runs, stale clears, calibration status, effect density, terminal preset, motion preset, glyph mode, throughput sample, and flush timing.

Snapshot mode prints deterministic visual summary JSON for regression checks:

```sh
cargo run -- --snapshot --profile benchmark --motion prime
```

Frame recording and scoring provide deterministic visual regression data:

```sh
cargo run -- --record-frames frames.jsonl --no-calibration
cargo run -- --score-frames frames.jsonl
```

The score reports `visual_quality_score`, flicker, lightness delta, motion continuity, clustering, foreground occlusion, role-aware contrast violations, dirty-cell pressure, dirty-budget violation rate, contrast-violation rate, foreground clarity, background atmosphere, temporal high-band energy, local flicker discomfort, phase coherence, stable foreground score, particle spectral quality, and glyph-topology metrics.

Golden recordings and ablation fixtures live under `tests/fixtures`. They cover the golden ultra look plus hash-only sampling, overbright palette, broken mouse flow, clumped particles, and contrast hierarchy regressions.

## Paper Experiments

The paper-oriented ablations expose the two proposed atomic contributions:

- `--dirty-mode naive|uniform-threshold|luminance-threshold|priority-dirty|topology-only|role-aware|full` compares raw cell diff, global thresholds, perceptual non-role baselines, priority dirty rendering, topology-only reconstruction, role-aware perceptual dirty predicates, and the complete renderer.
- `--glyph-history off|screen-cell|path|topology-dp` compares disabled line-glyph history, screen-cell history, path-indexed glyph history, and topology-constrained glyph reconstruction.
- `--fixed-dirty-budget <n>` overrides the dirty-cell budget used by score normalization.

Generate reproducible paper tables:

```sh
cargo run -- --paper-experiment ablations --output table_1_ablation_scores.csv --no-calibration
cargo run -- --paper-experiment dirty-budget --output table_2_dirty_budget_curves.csv --no-calibration
cargo run -- --paper-experiment terminal-matrix --output table_3_terminal_matrix.csv --no-calibration
cargo run -- --paper-experiment metric-validation --output metric_validation_table.csv --no-calibration
cargo run -- --paper-experiment glyph-reconstruction --output table_4_glyph_reconstruction.csv --no-calibration
cargo run -- --paper-experiment dirty-audit --output table_5_dirty_decision_audit.csv --no-calibration
cargo run -- --paper-experiment topology-metrics --output table_6_topology_metrics.csv --no-calibration
cargo run -- --paper-experiment topology-stress --output table_7_stress_topology_metrics.csv --no-calibration
cargo run -- --paper-experiment weight-sensitivity --output table_8_weight_sensitivity.csv --no-calibration
cargo run -- --paper-experiment runtime-complexity --output table_9_runtime_complexity.csv --no-calibration
cargo run -- --paper-experiment integrated-runtime --output table_10_integrated_runtime.csv --no-calibration
cargo run -- --paper-experiment speed-boundary --output table_11_speed_boundary.csv --no-calibration
cargo run -- --paper-experiment stress-interpretation --output table_12_stress_interpretation.csv --no-calibration
cargo run -- --paper-experiment weight-stability --output table_13_weight_stability_regions.csv --no-calibration
cargo run -- --paper-experiment translation-control --output table_14_translation_identity_control.csv --no-calibration
cargo run -- --paper-experiment runtime-degraded --output table_15_runtime_degraded_profile.csv --no-calibration
cargo run -- --paper-experiment weight-grid --output table_16_weight_grid.csv --no-calibration
cargo run -- --paper-experiment low-latency-quality-delta --output table_17_low_latency_quality_delta.csv --no-calibration
cargo run -- --paper-experiment low-latency-topology-metrics --output table_18_low_latency_topology_metrics.csv --no-calibration
cargo run -- --paper-experiment runtime-budget-ladder --output table_19_runtime_budget_ladder.csv --no-calibration
cargo run -- --paper-experiment terminal-io-protocol --output table_20_terminal_io_protocol.csv --no-calibration
cargo run -- --paper-experiment runtime-confidence-1200fps --output table_21_runtime_confidence_1200fps.csv --no-calibration
cargo run -- --paper-experiment adaptive-low-latency-policy --output table_22_adaptive_low_latency_policy.csv --no-calibration
python scripts/generate_paper_figures.py
```

`table_1_ablation_scores.csv` includes a deterministic `glyph-stress` fixture that isolates corner glyph instability. In that fixture, disabled glyph history produces high glyph flips, screen-cell history reduces them, and path-indexed history keeps the line glyphs stable.

The topology paper tables keep claims bounded: high-speed rows report
correspondence-loss detection rather than recovery, glyph robustness rows are
proxy evidence unless a measured terminal/font row is named, and integrated
runtime excludes terminal I/O.

`metric_validation_table.csv` scores golden and intentionally degraded frame
fixtures so the paper can justify each objective metric without requiring a
user-preference study.

## Compatibility

Recommended starting profiles:

| Terminal | Profile | Notes |
| --- | --- | --- |
| WezTerm | `ultra` | Truecolor and Unicode path should work well. |
| iTerm2 | `ultra` | Use a modern font with box drawing support. |
| Kitty | `ultra` | Good fit for high frame-rate rendering. |
| Alacritty | `cinematic` | Raise to `ultra` if benchmark tier stays stable. |
| Windows Terminal | `cinematic` | Use `TOKITAI_BRAILLE=1` only after checking font rendering. |
| VS Code terminal | `cinematic` | Integrated terminal may throttle high-rate output. |
| macOS Terminal | `calm` | Color and Unicode support are more conservative. |
| Linux console | `calm` | Basic color and ASCII fallback are expected. |
| Unknown SSH terminal | `calm` | Prefer `TOKITAI_ASCII=1` if glyphs look broken. |

See `docs/compatibility-capture.md` for the repeatable capture workflow.

Inspect mode reports the active pointer backend as `terminal-mouse`, `wayland-system`, `macos-system`, or another fallback with a confidence value. Wayland and macOS system pointer paths are treated as opportunistic assists; terminal mouse events remain the reliable baseline when the terminal provides them.

## Checks

```sh
cargo fmt -- --check
cargo test
cargo clippy -- -D warnings
cargo build --release
cargo run -- --snapshot
```
