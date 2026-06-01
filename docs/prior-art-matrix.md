# Prior-Art Matrix for Discrete Character-Cell Line-Glyph Reconstruction

Verdict date: 2026-06-01.
Review scope: public web, scholarly web pages, documentation pages, and database-style keyword searches. This artifact does not include authenticated ACM DL, IEEE Xplore, Scopus, Web of Science, or SpringerLink export files. `docs/prior-art-export-log.md` records the exact manual export gate. Before submission, the bibliography should be checked against those indexes using the query set below.

## Provisional Verdict

The strongest defensible claim is:

> To our knowledge, this is the first topology-constrained temporal reconstruction formulation for moving line primitives represented by a discrete character-cell glyph alphabet.

The claim should remain "to our knowledge" unless formal indexed-database exports are completed. The current review did not find a direct predecessor for metadata-backed temporal identity assignment of moving line primitives into a discrete terminal glyph alphabet. It did find adjacent work in pixel-space temporal antialiasing, terminal damage tracking, terminal image rendering, static ASCII/glyph approximation, vector stroke correspondence, and temporally coherent non-photorealistic line rendering.

## Formal Export Checklist

Run these searches in ACM DL, IEEE Xplore, SpringerLink, Scopus, and Web of Science before submission. Export title, authors, year, venue, DOI/URL, and abstract for the closest hits.

- `("character cell" OR "terminal graphics" OR "text user interface" OR "TUI") AND ("temporal coherence" OR "temporal stability" OR "animation")`
- `("ASCII art" OR "ANSI graphics" OR "box drawing" OR "character grid") AND ("line rendering" OR "vector rendering" OR "rasterization")`
- `("terminal UI" OR "text-mode graphics" OR "character-based display") AND ("dirty rectangle" OR "cell diff" OR "incremental rendering")`
- `("glyph identity" OR "topological consistency" OR "topology preservation") AND ("animation" OR "rendering" OR "line primitives")`
- `temporal antialiasing history reprojection validation`
- `temporal upsampling history reconstruction real time rendering`
- `ASCII art temporal coherence animation`
- `character cell rendering temporal coherence glyph`
- `terminal graphics glyph rendering temporal`
- `terminal damage tracking dirty cell rendering`
- `vector stroke correspondence animation temporal coherence`
- `temporally coherent line drawing non photorealistic rendering`
- `glyph topology temporal reconstruction`
- `discrete line drawing temporal coherence`

## Citation Shortlist

These are the minimum references to turn into formal bibliography entries. Exact DOI/export metadata should be filled from indexed databases where available.

| Ref key | Area | Candidate reference | Why it matters | Boundary for this paper |
| --- | --- | --- | --- | --- |
| YangTAA | Temporal antialiasing survey | Lei Yang et al., survey/tutorial material on temporal antialiasing and upsampling | Establishes pixel-space history, reprojection, and validation as known | Reconstructs pixels, not symbolic line-glyph topology |
| KarisTSS | Temporal supersampling | Brian Karis, High Quality Temporal Supersampling, 2014 | Canonical game-rendering TAA reference | Screen-space color history, no glyph identity |
| PlaydeadTAA | Production TAA | Playdead/INSIDE temporal reprojection presentations | Practical history validation and motion stability | Pixel framebuffer domain |
| Notcurses | Terminal damage rendering | Notcurses documentation and statistics | Terminal cell damage/output tracking | Tracks emitted cells, not moving primitive path identity |
| Tcell | Terminal cell diffing | `tcell` screen API/docs | TUI cell-state and draw synchronization | Cell diffing without topology-constrained glyph reconstruction |
| Ncurses | Terminal refresh optimization | ncurses virtual/physical screen refresh model | Classic terminal differential refresh | Screen update optimization, not temporal reconstruction |
| Chafa | Terminal image rendering | Chafa terminal graphics documentation/papers if available | Strong terminal glyph/image approximation neighbor | Image-to-glyph conversion, not path-indexed line identity |
| SixelKitty | Terminal pixel protocols | Sixel/Kitty graphics protocol documentation | Shows alternative terminal graphics path | Avoids text glyph topology by sending image payloads |
| Bresenham | Discrete line rasterization | Bresenham line drawing | Classic grid line construction | Per-frame rasterization, no temporal identity |
| WuLine | Antialiased line rasterization | Xiaolin Wu antialiased line drawing | Subcell line rendering in pixels | Pixel intensity, not character-glyph assignment |
| ASCIIArt | Structure-based ASCII art | Structure-based ASCII art / image-to-text rendering literature | Static glyph approximation neighbor | No moving primitive temporal identity |
| Animosaics | Mosaic/ASCII animation | Animosaics / temporally coherent mosaic animation literature | Temporal coherence in stylized elements | Not terminal line glyph topology after grid projection |
| NPRLine | Temporally coherent line art | Temporally coherent hatching / line drawing NPR literature | Temporal line coherence is known | Continuous surface/stroke domain, not terminal glyph alphabet |
| StrokeCorr | Vector stroke correspondence | Stroke correspondence for vector animation | Object/stroke identity over time | Vector output, not discrete terminal cells |
| DPSequence | Dynamic programming sequence alignment | Generic dynamic programming / sequence alignment references | Algorithmic tool background | Tool, not prior application to terminal line glyph reconstruction |

## Threat Matrix

| Source or area | Domain | Reconstruction object | History key | Glyph/topology awareness | Overlap | Difference | Threat |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Temporal antialiasing / temporal upsampling | Real-time rendering | Pixel samples | Screen-space reprojection, motion vectors, history buffers | No discrete glyph alphabet | Temporal history and validation | Operates on pixels, not topology-sensitive character glyph sequences | High |
| Terminal damage tracking | Terminal rendering | Terminal cells | Cell/plane output state | Glyphs exist but not path topology | Dirty-cell minimization | Tracks changed output, not moving primitive path identity | High |
| Terminal image renderers | Terminal graphics | Image approximated by symbols | Usually per frame | Glyph choice and dithering | Character-cell approximation | Image/video conversion, not path-indexed temporal line identity | High |
| ASCII art and mosaics | NPR / stylization | Static or animated stylized images | Object/image coherence depending on method | Character appearance | Temporal coherence can be an objective | Does not formulate terminal character-cell line-glyph topology reconstruction | Medium-High |
| Discrete line drawing | Rasterization | Grid cells or pixels | Per-frame geometry | Connectivity | Line discretization | No temporal glyph identity or finite terminal alphabet assignment | High |
| Vector stroke animation | Animation | Vector strokes/curves | Stroke IDs | Vector topology | Moving stroke identity | Output is vector strokes, not terminal cells | High |
| Glyph topology research | Typography | Font outlines | Static glyph structure | Yes, font-internal | Glyph topology reasoning | Not temporal assignment of moving terminal line glyphs | Medium |
| Dynamic programming assignment | Algorithms | Ordered sequences | Sequence index | Generic order | Ordered identity assignment | Algorithmic primitive, not terminal graphics prior work | Low |

## Claim Wording

Use:

> To our knowledge, this is the first topology-constrained temporal reconstruction formulation for moving line primitives represented by a discrete character-cell glyph alphabet.

Avoid:

- "first temporal antialiasing method for terminals";
- "first terminal renderer with dirty rendering";
- "first glyph topology method";
- "first character-cell renderer";
- "terminal compatibility as the contribution".

## If a Near Predecessor Is Found

Downgrade to:

> We adapt topology-constrained temporal assignment to the under-studied case of terminal character-cell line glyphs and provide metadata-backed stress evaluation for path-indexed glyph identity.
