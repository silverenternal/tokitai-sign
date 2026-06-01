# Compatibility Capture Workflow

Use a fixed terminal size, font, profile, motion preset, and theme for every capture.

## Current Paper Blocker

`table_3_terminal_matrix.csv` currently contains an auto-generated environment
row plus terminal templates. This is sufficient for artifact reproduction, but
not sufficient for a cross-terminal claim. Before submission, replace the
template rows with measured rows from at least:

- WezTerm
- Kitty or Alacritty
- VS Code integrated terminal

Use the same font family, font size, terminal columns, terminal rows, profile,
motion preset, and theme for every row. Keep failed or degraded captures and
describe the failure mode in `notes`; do not discard them.

Recommended baseline:

```sh
cargo run -- --snapshot --profile benchmark --motion prime --theme examples/tokitai.theme
cargo run -- --no-loader --profile benchmark --motion prime --record metrics.log
cargo run -- --record-frames frames.jsonl --no-calibration
cargo run -- --score-frames frames.jsonl
```

Record these fields with each terminal capture:

- Terminal emulator and version
- Operating system
- Font family and size
- Terminal size in columns and rows
- Pixel resolution or window size when available
- Git commit and command line
- Profile and motion preset
- Theme path
- Snapshot JSON
- Visual score JSON
- Benchmark FPS tier, missed deadlines, dirty cells, ANSI runs, glyph mode, preset name, and throughput sample
- Whether the six-layer cosmic background keeps visible differential motion
- Whether left-button mouse drag creates a distinct particle wake without pulling the background particle field

Update README compatibility notes only from captured evidence.

## Real-World Rendering Figure

For the paper-facing screenshot, capture the canonical real-orbit scene in a
named terminal emulator and use a caption with this exact metadata shape:

> Captured on `[Terminal Name version]` with `[Font family size]`,
> `[columns]x[rows]` cells, `[pixel resolution if known]`, command
> `cargo run -- --no-loader --profile benchmark --motion prime --glyph-history topology-dp`,
> git commit `[hash]`, captured on `[date]`.

The screenshot is qualitative evidence for glyph connectivity and visual
appearance. It does not prove an end-to-end 1200 FPS display rate.

## Terminal I/O and Display-Cadence Capture

Use `table_20_terminal_io_protocol.csv` as the field checklist. Capture three
separate scopes:

- Frame construction: use generated runtime tables. This supports only
  frame-build claims.
- Terminal write/flush: record terminal, shell or tmux state, font, grid size,
  bytes per frame, write time, and flush time.
- Observed display cadence: record monitor refresh rate, capture method,
  capture FPS, delivered frame intervals, and dropped frames.

Slow-motion phone video or screen recording may be attached as qualitative
evidence, but do not treat it as the primary quantitative FPS measurement. If
observed display cadence is below 1200 FPS, keep the paper wording as:

> Frame construction supports the 1200 FPS frame-build budget; delivered
> display cadence is bounded by terminal I/O, compositor scheduling, and
> monitor refresh.

## Paper Terminal Matrix Capture

Use this workflow when filling `table_3_terminal_matrix.csv`.

Run the same capture set in each terminal emulator:

```sh
cargo run -- --snapshot --profile benchmark --motion prime --no-calibration
cargo run -- --paper-experiment terminal-matrix --output /tmp/tokitai-terminal-row.csv --no-calibration
cargo run -- --paper-experiment ablations --output /tmp/tokitai-ablation.csv --no-calibration
cargo run -- --paper-experiment dirty-budget --output /tmp/tokitai-budget.csv --no-calibration
cargo run -- --no-loader --profile benchmark --motion prime --dirty-mode full --glyph-history path --record /tmp/tokitai-metrics.log
```

For glyph-history captures, record three short clips under identical terminal
size and font settings:

```sh
cargo run -- --record-frames /tmp/glyph-off.jsonl --dirty-mode full --glyph-history off --no-calibration
cargo run -- --record-frames /tmp/glyph-screen.jsonl --dirty-mode full --glyph-history screen-cell --no-calibration
cargo run -- --record-frames /tmp/glyph-path.jsonl --dirty-mode full --glyph-history path --no-calibration
```

Fill these `table_3_terminal_matrix.csv` columns from the same run:

- `terminal`
- `os`
- `font`
- `columns`
- `rows`
- `profile`
- `motion`
- `dirty_mode`
- `glyph_history`
- `fps_tier`
- `average_fps`
- `worst_frame_ms`
- `dirty_cells`
- `dirty_runs`
- `stale_cells`
- `stale_runs`
- `glyph_mode`
- `terminal_preset`
- `color_mode`
- `notes`

Do not mix values from different terminal sizes, fonts, or profiles in the same
row. If a terminal cannot hold the requested FPS tier, keep the row and note the
failure mode instead of discarding it.
