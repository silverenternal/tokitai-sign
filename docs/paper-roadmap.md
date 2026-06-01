# Paper Roadmap

## Goal

Turn `tokitai-sign` from an impressive engineered terminal animation into a
paper-worthy method. The paper should be about perceptual dirty rendering for
character-cell animation, not about a collection of visual effects.

## Research Question

Can a role-aware perceptual dirty renderer improve perceived smoothness and
glyph stability for high-frame-rate terminal animation under the same ANSI
output budget as naive dirty rendering?

## What Must Become More Original

### 1. Formalize the Perceptual Dirty Model

Current state:

- The implementation has role-aware thresholds, contrast floors, and temporal
  scoring.

Needed:

- Define the dirty decision as a role-weighted error function.
- Make thresholds explicit and measurable.
- Add one command or test path that compares naive dirty rendering against
  perceptual dirty rendering on the same frame sequence.

Deliverable:

- A method section with equations and a reproducible ablation.

### 2. Strengthen Glyph-Topology Reconstruction

Current state:

- Orbit history now uses path-indexed temporal state and glyph hold.

Needed:

- Add an ablation that disables path-indexed history.
- Measure glyph flips per second and orbit continuity.
- Generalize the code comments and metric names from "orbit" toward
  "glyph-topology motion" where appropriate.

Deliverable:

- A table showing lower glyph instability at the same dirty-cell budget.

### 3. Add Budget-Constrained Rendering Experiments

Current state:

- The renderer reports dirty-cell pressure and adaptive quality.

Needed:

- Run the same animation at fixed budgets: 160, 240, 320, 480, and 640 dirty
  cells per frame.
- Compare naive dirty, role-aware dirty, and full temporal reconstruction.
- Report quality versus budget curves.

Deliverable:

- A figure showing that the proposed method dominates naive dirty rendering
  at low and medium budgets.

### 4. Add Cross-Terminal Capture

Current state:

- The README defines compatible terminals and a capture workflow.

Needed:

- Capture at least three terminals with different behavior: WezTerm, Kitty or
  Alacritty, and VS Code terminal.
- Record FPS tier, dirty cells, run count, and any glyph/color issues.

Deliverable:

- A terminal matrix table.

### 5. Validate Objective Graphics Metrics

Current state:

- The repository has deterministic visual scores and controlled regression
  fixtures.

Needed:

- Show that each degraded fixture is penalized by the intended metric.
- Keep the aggregate score non-saturated so ablations remain distinguishable.
- Report raw metrics next to the aggregate score.

Deliverable:

- `metric_validation_table.csv` with golden, hash-only, overbright, broken
  mouse flow, clumped particle, and contrast-hierarchy rows.

## Implementation Tasks

1. Add `--dirty-mode naive|uniform-threshold|role-aware|full`.
2. Add `--glyph-history off|screen-cell|path`.
3. Add `--fixed-dirty-budget <n>` for deterministic experiments.
4. Extend `VisualScore` with:
   - glyph_flips_per_second
   - orbit_cells_changed_per_frame
   - dirty_budget_violation_rate
   - foreground_readability_min
5. Add frame fixtures for each ablation.
6. Add a script or cargo subcommand that produces CSV experiment tables.

## Paper Figures

1. Method diagram:
   cell role -> perceptual error -> dirty decision -> terminal output.
2. Orbit reconstruction diagram:
   sub-cell path position -> path index history -> stable glyph/color output.
3. Quality versus dirty budget curve.
4. Glyph instability ablation bar chart.
5. Cross-terminal result table.
6. Objective metric validation table.

## SCI/Q2 Potential

The project has Q2 SCI potential only if it becomes a reproducible graphics
method paper with ablations, objective metric validation, and cross-terminal
evidence. As a visual engineering demo, it is not enough.

Most realistic positioning:

- Applied visualization
- Multimedia tools
- HCI-adjacent interactive systems
- Software practice with empirical evaluation

Less realistic positioning:

- Pure computer graphics algorithm paper
- Top-tier systems paper

## Acceptance Risk

High risk:

- Reviewers may say the method is an engineering combination of known TAA,
  blue-noise sampling, dirty rendering, and color-space tricks.

Risk reduction:

- Make the novelty the terminal-specific formulation:
  role-aware perceptual dirty rendering under an ANSI output budget.
- Prove that this formulation changes decisions and improves quality under
  fixed budgets.
- Provide ablations that isolate each proposed component.

## Minimum Paper-Ready Bar

The work becomes paper-ready when the repository can produce:

```text
table_1_ablation_scores.csv
table_2_dirty_budget_curves.csv
table_3_terminal_matrix.csv
table_4_glyph_reconstruction.csv
table_5_dirty_decision_audit.csv
metric_validation_table.csv
figure_1_method.png
figure_2_quality_budget_curve.png
figure_3_metric_validation.png
figure_4_glyph_reconstruction.png
```

and when the results support the central claim:

> Role-aware perceptual dirty rendering improves terminal animation smoothness
> and glyph stability under fixed ANSI output budgets.

## Implemented Experiment Commands

The current repository can generate the first two CSV artifacts directly:

```sh
cargo run -- --paper-experiment ablations --output table_1_ablation_scores.csv --no-calibration
cargo run -- --paper-experiment dirty-budget --output table_2_dirty_budget_curves.csv --no-calibration
cargo run -- --paper-experiment terminal-matrix --output table_3_terminal_matrix.csv --no-calibration
cargo run -- --paper-experiment metric-validation --output metric_validation_table.csv --no-calibration
cargo run -- --paper-experiment glyph-reconstruction --output table_4_glyph_reconstruction.csv --no-calibration
cargo run -- --paper-experiment dirty-audit --output table_5_dirty_decision_audit.csv --no-calibration
python scripts/generate_paper_figures.py
```

Mode-specific recordings can be produced with:

```sh
cargo run -- --record-frames /tmp/naive.jsonl --dirty-mode naive --glyph-history off --no-calibration
cargo run -- --record-frames /tmp/role-aware.jsonl --dirty-mode role-aware --glyph-history off --no-calibration
cargo run -- --record-frames /tmp/path.jsonl --dirty-mode full --glyph-history path --no-calibration
cargo run -- --score-frames /tmp/path.jsonl
```

The terminal matrix template is stored in:

```text
table_3_terminal_matrix.csv
```

`table_1_ablation_scores.csv` also includes `glyph-stress` rows. These rows are
a deterministic synthetic fixture that isolates corner glyph instability:

- `glyph-history off` intentionally flips corner glyphs every frame.
- `glyph-history screen-cell` reduces but does not remove the instability.
- `glyph-history path` keeps glyph identity stable.

This fixture exists to make the path-indexed glyph reconstruction claim
measurable instead of relying on the normal animation clip, where the difference
can be visually subtle.

`metric_validation_table.csv` validates the objective metric suite against
controlled degradations. It is the required replacement for a user preference
study in the graphics-method framing. A subjective study can still be added as
appendix or future work, but it is no longer a core paper dependency.

`table_4_glyph_reconstruction.csv` is the main hard-claim table. It separates
topology-derived real orbit-corner motion from the synthetic glyph-stress
fixture and compares disabled, screen-cell, and path-indexed history.

`table_5_dirty_decision_audit.csv` supports the secondary dirty-rendering claim
by reporting emitted, suppressed, stale-cleared, and budget-dropped cells per
perceptual role and baseline.
