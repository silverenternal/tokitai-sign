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

- Spatiotemporal blue-noise style hashing for particle visibility and shimmer.
- Perceptual error fields to keep stochastic particles away from the logo and orbit focus area.
- Velocity-aligned anisotropic sampling for directional particle trails.
- A low-resolution vorticity-preserving velocity grid for mouse-driven particle wake motion.
- Cell-space temporal anti-aliasing with role and color-delta rejection.
- OKLCH constrained color optimization with separate contrast floors for logo, status text, orbit, mouse trail, background particles, and afterimages.
- Role-aware visual scoring for foreground clarity, background atmosphere, flicker, clustering, and contrast hierarchy.

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

The score reports `visual_quality_score`, flicker, lightness delta, motion continuity, clustering, foreground occlusion, role-aware contrast violations, dirty-cell pressure, foreground clarity, and background atmosphere.

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

## Checks

```sh
cargo fmt -- --check
cargo test
cargo clippy -- -D warnings
cargo build --release
cargo run -- --snapshot
```
