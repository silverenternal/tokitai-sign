# Compatibility Capture Workflow

Use a fixed terminal size, font, profile, motion preset, and theme for every capture.

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
- Profile and motion preset
- Theme path
- Snapshot JSON
- Visual score JSON
- Benchmark FPS tier, missed deadlines, dirty cells, ANSI runs, glyph mode, preset name, and throughput sample
- Whether the six-layer cosmic background keeps visible differential motion
- Whether left-button mouse drag creates a distinct particle wake without pulling the background particle field

Update README compatibility notes only from captured evidence.
