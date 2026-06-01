# Baseline Suite

The glyph/topology experiments use baselines chosen to avoid a weak comparison
against deliberately broken variants.

| Baseline | Implementation mode | What it represents |
| --- | --- | --- |
| Per-frame glyph rasterization | `glyph-history off` | A vector-to-cell or glyph renderer that chooses characters independently each frame. |
| Screen-space history | `glyph-history screen-cell` | A TAA-like or dirty-state method keyed by terminal coordinates. |
| Path-indexed cache | `glyph-history path` | A simple topology-indexed cache without explicit assignment optimization. |
| Topology-constrained assignment | `glyph-history topology-dp` | The proposed method, adding local ordered assignment costs. |
| Dirty-only topology isolation | `dirty-mode topology-only` | Separates glyph reconstruction from the full dirty scheduler. |

## Reviewer Baseline Positioning

`table_24_baseline_positioning.csv` maps the implemented baselines to
reviewer-recognizable categories. The goal is to avoid a strawman comparison:
each baseline corresponds to a plausible design point or adjacent literature
area.

| Category | Paper role | Why it is fair | Why it is limited |
| --- | --- | --- | --- |
| Standard grid line rasterization | `glyph-history off` | Represents independent per-frame glyph choice. | Has no temporal identity state. |
| Coordinate-keyed temporal history | `glyph-history screen-cell` | Mirrors a natural screen-space history design. | A moving primitive can leave the coordinate while keeping the same path identity. |
| Path-keyed cache | `glyph-history path` | Tests whether path identity alone is enough. | Does not optimize neighbor topology or corner identity. |
| TUI damage rendering | dirty modes and related work | Represents terminal update optimization. | Optimizes output bandwidth, not symbolic line identity. |
| Terminal image/glyph rendering | related work only | Represents image-to-character approximation systems. | Reconstructs image frames, not path-indexed line primitives. |
| Pixel TAA and vector stroke coherence | related work only | Represents temporal coherence in nearby domains. | Works in pixel/vector domains, not a finite terminal glyph alphabet. |

## Reviewer-Facing Rationale

The comparison intentionally spans both nearby traditions:

- pixel/TAA-style history is represented by screen-space history;
- terminal/image per-frame conversion is represented by per-frame glyph
  rasterization;
- a simpler version of the proposed idea is represented by path-indexed cache;
- the proposed method is the only mode with topology-constrained assignment.

This makes the central experiment about reconstruction identity, not about
theme choices, terminal throughput, or visual polish.

The adjacent systems are not dismissed as weak. They solve different problems:
dirty renderers minimize terminal output, terminal image renderers approximate
image frames, and pixel/vector temporal methods preserve samples or strokes in
continuous domains. They become direct baselines only if they are modified to
emit path-indexed topology-sensitive line glyph identities, which is precisely
the formulation studied here.
