# Prior-Art Export Log

Date: 2026-06-01.

This log separates locally reproducible claim-hardening from authenticated
index exports. The current environment cannot prove institutional access to
ACM DL, IEEE Xplore, Scopus, Web of Science, or SpringerLink export tools, so
this artifact does not fabricate formal export files or hit counts.

## Query Set

Use the same query set in each indexed database:

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

## Export Table

| Database | Access status in this artifact | Query date | Export status | Required record before submission |
| --- | --- | --- | --- | --- |
| ACM Digital Library | not authenticated here | 2026-06-01 | pending manual export; not claim-ready | query string, hit count, closest 5 records, DOI/URL, abstract |
| IEEE Xplore | not authenticated here | 2026-06-01 | pending manual export; not claim-ready | query string, hit count, closest 5 records, DOI/URL, abstract |
| Scopus | not authenticated here | 2026-06-01 | pending manual export; not claim-ready | query string, hit count, closest 5 records, DOI/URL, abstract |
| Web of Science | not authenticated here | 2026-06-01 | pending manual export; not claim-ready | query string, hit count, closest 5 records, DOI/URL, abstract |
| SpringerLink | not authenticated here | 2026-06-01 | pending manual export; not claim-ready | query string, hit count, closest 5 records, DOI/URL, abstract |

## Public Web Cross-Check

Public web search on 2026-06-01 did not find a direct predecessor for
topology-constrained temporal reconstruction of moving line primitives into a
discrete character-cell glyph alphabet. The closest public hits remain adjacent
rather than equivalent:

- structure-based ASCII art: optimizes character choice for image structure
  and explicitly discusses animation without explicit temporal coherence;
- terminal image/video renderers: use glyph shape matching, delta rendering, or
  synchronized output to reduce flicker, but reconstruct image cells rather than
  path-indexed line-glyph identity;
- temporally coherent NPR line/art animation: preserves stroke or stylized
  image coherence in vector/image domains, not terminal character-cell line
  glyph topology;
- terminal dirty rendering systems: optimize cell emission and refresh but do
  not formulate topology-constrained temporal glyph assignment.

This public check supports the bounded "to our knowledge" claim, but it is not
a substitute for authenticated indexed database exports.

## Related-Work Boundary Notes

Use these distinctions when writing the final Related Work section:

- Static ASCII art and vector-to-text conversion solve static approximation or
  image stylization; they do not model temporal glyph identity for moving line
  primitives.
- TUI and terminal rendering libraries solve dirty-region updates, cell
  emission, and terminal throughput; they do not solve topology-constrained
  glyph assignment for projected primitive motion.
- Pixel-space temporal antialiasing and vector temporal coherence preserve
  samples or strokes in continuous image/vector domains; they do not operate on
  a finite discrete glyph alphabet with path-keyed line identity.
- Terminal image/video renderers approximate image frames with glyphs or color
  cells; they do not maintain metadata-backed path identity for a moving line
  primitive.

## Claim Rule

Until those exports are attached, the manuscript must use:

> To our knowledge, this is the first topology-constrained temporal
> reconstruction formulation for moving line primitives represented by a
> discrete character-cell glyph alphabet.

If an indexed near predecessor is found, downgrade to:

> We adapt topology-constrained temporal assignment to the under-studied case
> of terminal character-cell line glyphs and provide metadata-backed stress
> evaluation for path-indexed glyph identity.
