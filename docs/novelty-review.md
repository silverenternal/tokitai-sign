# Novelty Review: Topology-Constrained Character-Cell Line-Glyph Reconstruction

## Verdict

Current claim strength is conditional-strong after the broader public
database-style search, reframing, and implementation of `topology-dp`. The paper
should still use "to our knowledge" rather than an unconditional "first" claim
unless authenticated database exports are checked before submission.

Recommended claim:

> To our knowledge, this is the first topology-constrained temporal
> reconstruction formulation for moving line primitives represented by a
> discrete character-cell glyph alphabet.

## Why This Is Stronger Than the Earlier Claim

The earlier framing could be read as terminal engineering: dirty rendering,
perceptual thresholds, and path-indexed caching. The revised framing defines a
specific reconstruction object and a specific algorithmic assignment problem:

- the output is a symbolic line glyph, not a pixel;
- the temporal identity is stored on the moving path, not the terminal cell;
- emitted topology fixtures carry explicit `primitive_id` and `path_index`
  metadata, so path-order metrics are data-derived rather than fixture-name
  rules;
- the proposed `topology-dp` mode minimizes temporal, local, topology, and
  corner-identity costs;
- the main metrics directly measure topology breaks, path-order violations,
  corner identity flips, and connected-component instability.

## Prior-Art Boundary

The detailed source matrix and search log are in `docs/prior-art-matrix.md`.
The short boundary is:

| Adjacent area | Known | Why the revised claim remains different |
| --- | --- | --- |
| Temporal antialiasing | Pixel-space history reprojection and validation are known. | This work assigns symbolic glyph identity on a path topology before cell projection. |
| Dirty/damage rendering | Cell/region update minimization is known. | Dirty updates are not the novelty; topology-constrained reconstruction is. |
| Terminal image renderers | Per-frame glyph/image approximation is known. | The target is temporal coherence of moving line glyphs, not static image approximation. |
| Structure-based ASCII art | Static image structure can be mapped to ASCII glyphs. | This work reconstructs temporal identity for moving line primitives. |
| Temporally coherent NPR line/stroke methods | Continuous strokes can be kept coherent over time. | This work projects path identity into a discrete terminal glyph alphabet. |
| Vector stroke animation | Object/stroke identity is known. | The hard output constraint is a discrete terminal glyph alphabet. |
| Glyph topology research | Glyph shape/topology analysis is known. | The work reconstructs moving line-glyph sequences, not font outlines. |

## Strong Claim Conditions

The claim is strong only if all of the following stay true:

- `topology-dp` is presented as the proposed method.
- `path` is presented as a simpler baseline, not the final algorithm.
- Table 6 and Figure 5 are the first result artifacts discussed.
- Table 7 reports both stress degradation and correspondence-loss detection.
- Cross-terminal evidence is treated as robustness, not novelty.
- The paper avoids broad novelty claims about TAA, dirty rendering, perceptual
  rendering, terminal graphics, or glyph topology in general.

## Remaining Novelty Risk

The highest-risk unknown is still prior work behind authenticated databases or
non-obvious terminology in symbolic animation/vector-to-glyph temporal
coherence. If such work exists, the claim should be weakened to:

> We adapt topology-constrained temporal assignment to the previously
> under-studied case of terminal character-cell line glyphs.

Until that risk is fully resolved, "to our knowledge" is the correct strength.
The public search did not find a direct match.
