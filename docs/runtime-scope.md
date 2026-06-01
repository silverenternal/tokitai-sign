# Runtime Scope

The runtime evidence is scoped to release frame construction and topology
assignment overhead. It intentionally excludes terminal flush latency, terminal
emulator scheduling, shell/tmux forwarding, and display compositor latency.

## Current Evidence

`table_10_integrated_runtime.csv` reports
`release-frame-build-no-terminal-io`. In the current artifact, the integrated
full-visual frame build fits a 60 FPS frame-build budget but exceeds a 120 FPS
budget. `table_15_runtime_degraded_profile.csv` now measures an explicit
low-latency topology-dp path. That path skips high-cost particle background,
afterimage, animated logo material, temporal color AA, and smoothing while
retaining the topology-dp orbit reconstruction. On the current machine it fits
the 120 FPS frame-build budget with large headroom. `table_17` now separates
runtime-budget quality from visual richness, so low-latency is not presented as
visually identical or globally superior. The medium-latency profile keeps the
afterimage history effect while skipping heavier visual effects and lands
between full-visual and low-latency in the runtime ladder. `table_19` places
the profiles against 60, 120, 240, 1000, and 1200 FPS frame-build budgets, and
`table_21` reports a longer low-latency 1200 FPS confidence run.

This means the paper can claim 1200 FPS frame-build budget support only for
the measured low-latency topology-dp row on the measured machine, and only if
the p99 row remains under 833.33 us/frame. It still cannot claim 1200 FPS
terminal I/O or display refresh, because terminal flush latency, emulator
scheduling, shell/tmux forwarding, and display compositor latency are not
included.

`table_9_runtime_complexity.csv` reports fixed-candidate assignment timing and
supports the linear-in-visible-path-samples implementation claim. It is a
microbenchmark for the assignment component, not an end-to-end renderer result.

## 1200 FPS Frame-Build Policy

For high-FPS frame-build targets, the artifact should use the measured
low-latency profile. The implemented degradation strategy is:

- keep topology-dp for the orbit reconstruction;
- use a medium-latency profile that keeps afterimage history while skipping
  particle background, animated logo material, temporal color AA, and smoothing;
- use a low-latency profile that additionally skips afterimage history;
- activate low-latency rendering adaptively after repeated frame-build
  overloads;
- optionally run with `--uncapped`, which disables active frame sleeping and
  uses low-latency rendering.

`--uncapped` means the renderer does not intentionally sleep between frames. It
does not mean unlimited visible FPS: terminal throughput, terminal refresh, CPU
scheduling, and display refresh remain external hard limits.

No one-command artifact currently measures terminal I/O. End-to-end terminal
latency requires a separate capture protocol with terminal emulator, shell/tmux
state, font, window size, and flush timing recorded.

## Single-Machine Boundary

The current runtime rows are measured on one local environment. They are valid
artifact measurements, but they are not hardware-independent performance
guarantees. Until `table_19_runtime_budget_ladder.csv` and
`table_21_runtime_confidence_1200fps.csv` are repeated on another CPU/OS
environment, use "on the measured machine" or "in the measured artifact run"
when discussing the 60/240/1200 FPS frame-build tiers.
