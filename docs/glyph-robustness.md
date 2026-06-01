# Glyph Robustness Capture Notes

The current artifact contains controlled glyph-alphabet proxy evidence in
`table_8_weight_sensitivity.csv`:

- `unicode`: box-drawing glyph alphabet;
- `ascii`: ASCII fallback for strokes and corners;
- `degraded`: intentionally ambiguous synthetic stroke alphabet.

These rows are useful for testing whether the canonical result depends only on
one Unicode box-drawing representation. They are not a real multi-font study.

## Local Measured Environment Row

This artifact now records one local terminal-layer measurement context:

- terminal layer: `tmux`;
- `TERM`: `tmux-256color`;
- color mode: `truecolor`;
- OS: `Linux 7.0.7-zen2-1-zen x86_64`;
- terminal size used by artifact commands: 80 columns by 24 rows;
- local fontconfig monospace match: `Nimbus Mono PS Regular`;
- glyph mode reported by the artifact: Unicode;
- box-drawing connectivity: not visually certified from a screenshot in this
  row;
- caveat: tmux does not identify the outer terminal emulator or its actual
  rendered font, so this is a measured terminal-layer row, not a broad
  cross-font proof.

This row supports the weaker statement that the artifact was regenerated in a
documented local terminal-layer configuration. It does not support
font-independent or cross-terminal claims.

## Required Manual Rows Before Submission

For a stronger SCI submission, add at least one measured row using a real
terminal/font configuration. This remains a manual gate because the current
workspace can identify the tmux layer but cannot visually certify the outer
terminal emulator or rendered font:

```sh
cargo run --release -- --paper-experiment topology-metrics --output table_6_topology_metrics.csv --no-calibration
cargo run --release -- --paper-experiment topology-stress --output table_7_stress_topology_metrics.csv --no-calibration
```

Record:

- terminal emulator;
- OS and version;
- font family and size;
- columns and rows;
- glyph mode;
- whether box-drawing characters render as connected strokes;
- any observed glyph fallback or spacing problems.
- screenshot or recording path;
- capture date and display scale if available.

Add the measured environment to `table_3_terminal_matrix.csv` or a dedicated
appendix table. Until multiple visual captures are measured, manuscript claims
must say "proxy glyph-alphabet robustness plus one documented local
terminal-layer row" rather than "font-independent robustness."

## Screenshot Acceptance Criteria

A screenshot row is acceptable only if it records the outer emulator and actual
font reported by that emulator or by its configuration. A tmux-only capture is
not enough unless the outer emulator is also named. Box-drawing connectivity
should be marked as `connected`, `gapped`, or `unknown`, with the screenshot
path referenced from `ARTIFACT.md`.

## Fixture Generalization Appendix

The fixture suite is designed to prevent over-reading the canonical zero-flip
rows. Each fixture covers a different threat to temporal glyph identity:

| Fixture | What it tests | What it does not prove |
| --- | --- | --- |
| `real-orbit` | A realistic moving orbit line around the logo, with path-keyed identity under normal motion. | It is still a controlled trajectory, not a proof for arbitrary line motion. |
| `glyph-stress` | Dense corner and stroke transitions inside a compact character grid. | It does not cover physical terminal/font rendering differences. |
| `canonical` | Recoverable reference behavior for stress comparisons. | It is not a robustness result by itself. |
| `shuffled-path` | Metadata-derived path-order detection under corrupted path order. | It is not a visual rendering failure case. |
| `high-speed` | Correspondence loss when motion exceeds the local candidate window. | It does not claim high-speed recovery. |
| `topology-break` | Temporary removed cells and topology mutation. | It does not claim hidden repair of true topology changes. |
| `line-crossing` | Ambiguous crossing regions. | It does not claim universal crossing resolution. |
| `bounded-jitter` | Small perturbations inside the recoverable operating envelope. | It does not cover unbounded random motion. |
| `shape-continuity` | Whether neighbor and corner costs separate `topology-dp` from weaker history modes. | It is still synthetic and should be read with the stress table. |

The canonical zero-flip rows support the bounded recoverable-correspondence
claim. The stress rows are equally important: they show where the method
detects loss, degrades, or reports ambiguity rather than pretending to solve
all motion.
