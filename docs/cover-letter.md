# Cover Letter Draft

Date: 2026-06-01.

Target journal placeholder: `[Target Journal Name]`.

## Core Letter

Dear Editor,

We are pleased to submit our manuscript entitled "Topology-Constrained
Temporal Reconstruction for Line Primitives in Discrete Character Grids" for
consideration in `[Target Journal Name]`.

This work studies an under-addressed reconstruction problem in terminal
character-cell graphics: moving line primitives must be represented by discrete
glyph sequences while preserving temporal identity and local topology. Existing
terminal rendering systems primarily focus on cell updates, dirty-region
optimization, or static character-art conversion, but they do not explicitly
model topology-preserving temporal glyph identity for moving line primitives.
To our knowledge, this is the first work to formulate this problem as
topology-constrained temporal reconstruction on a discrete character grid.

Our core contribution is `topology-dp`, a local fixed-candidate assignment
method that scores temporal identity, path proximity, neighbor topology, and
corner identity. On controlled canonical fixtures within recoverable
correspondence bounds, the method eliminates measured glyph identity flips and
corner identity flips, while stress fixtures explicitly expose
correspondence-loss and topology-mutation boundaries. The artifact includes
reproducible metadata-backed metrics, sensitivity analysis, stress tests, and
scoped runtime measurements. As supporting systems evidence, the measured
low-latency profile satisfies a 1200 FPS frame-build budget on the artifact
machine, while end-to-end terminal display refresh remains explicitly outside
the claim.

We believe this work contributes a bounded but reproducible method for
topology-stable character-cell line rendering, with relevance to computer
graphics, visualization, and real-time terminal user interfaces.

Sincerely,

`[Author Names]`

## Phrases To Keep

- "under-addressed reconstruction problem"
- "To our knowledge"
- "within recoverable correspondence bounds"
- "1200 FPS frame-build budget"
- "end-to-end terminal display refresh remains outside the claim"

## Phrases To Avoid

- "unsolved challenge"
- "terminal animation reaches 1200 FPS"
- "unlimited FPS"
- "visually identical low-latency mode"
- "font-independent"
