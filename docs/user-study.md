# User Preference Study Template

## Purpose

Validate whether the proposed perceptual dirty renderer and path-indexed glyph
history are perceived as smoother, more readable, and less distracting than
baseline terminal rendering modes.

## Conditions

Prepare clips from these modes:

1. `--dirty-mode naive --glyph-history off`
2. `--dirty-mode uniform-threshold --glyph-history off`
3. `--dirty-mode role-aware --glyph-history off`
4. `--dirty-mode full --glyph-history screen-cell`
5. `--dirty-mode full --glyph-history path`

## Clip Generation

```sh
cargo run -- --record-frames /tmp/clip-naive.jsonl --dirty-mode naive --glyph-history off --no-calibration
cargo run -- --record-frames /tmp/clip-role-aware.jsonl --dirty-mode role-aware --glyph-history off --no-calibration
cargo run -- --record-frames /tmp/clip-path.jsonl --dirty-mode full --glyph-history path --no-calibration
```

Replay and capture each clip with the same terminal size, font, profile, and
screen recorder settings.

## Questions

For each randomized pair:

1. Which clip appears smoother?
2. Which clip keeps the logo and orbit easier to read?
3. Which clip is less visually distracting?
4. Did you notice line-glyph flicker or corner instability?

Use forced choice plus an optional short comment.

## Minimum Sample

- Minimum: 12 participants.
- Better: 24 participants.
- Each participant should evaluate 8 to 12 randomized clip pairs.

## Analysis

For each comparison, report:

- preference count
- preference percentage
- binomial confidence interval
- free-text themes

The paper should not claim subjective superiority unless the proposed mode wins
the smoother/readable/less-distracting choices with a clear margin.
