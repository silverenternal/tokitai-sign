# Target Journal Shortlist

This shortlist is a positioning aid, not a final ranking. Current quartiles and
indexing must be rechecked immediately before submission because journal
metrics change.

Verification date for this draft: 2026-06-01.

Source notes checked for this draft:

- Multimedia Tools and Applications journal page:
  `https://www.springer.com/journal/11042`
- Recent public ranking/indexing summaries for Multimedia Tools and
  Applications list Media Technology as Q1 and several computer-science
  categories as Q2. Treat these as submission-planning evidence, not as a
  substitute for final JCR/SJR verification.
- Public scope summaries for The Visual Computer describe computer graphics and
  visual computing coverage, which makes it a higher-risk but relevant
  graphics-method candidate.

## Recommended Primary Framing

Frame the work as a reproducible multimedia/graphics tool paper:

- primary idea: path-indexed temporal reconstruction for terminal line-glyph
  motion;
- secondary method: role-aware perceptual dirty scheduling under ANSI budgets;
- evidence: deterministic ablations, glyph reconstruction fixtures,
  dirty-budget curves, dirty-decision audit, metric-validation fixtures, and
  cross-terminal captures;
- limitation: no broad claim beyond character-cell line-glyph animation.

## Primary Target Candidate

| Venue | Current positioning | Fit | Main risk |
| --- | --- | --- | --- |
| Multimedia Tools and Applications | Public indexing pages list recent SCImago quartile information and the journal scope covers multimedia tools, systems, and applications. | Best match for a reproducible system/method paper with empirical artifact evidence. | The manuscript must avoid reading like a single terminal demo; it needs clear method, baselines, and artifact packaging. |

Why this is the pragmatic primary target:

- The current contribution is tool/method oriented rather than a broad graphics
  theory paper.
- The deterministic artifact package, benchmark tables, and reproducibility
  script are central strengths.
- The venue framing can accept a narrow applied system if the evaluation is
  disciplined.

## Secondary Candidates

| Venue | Fit | Risk |
| --- | --- | --- |
| The Visual Computer | Stronger graphics-method framing if the manuscript emphasizes discrete reconstruction and line-glyph topology. | Higher risk unless related work and figures are polished and cross-terminal evidence is complete. |
| Journal of Visualization | Possible fit if the paper is reframed around character-cell visualization and readability. | Needs a clearer visualization task beyond terminal animation quality. |
| SoftwareX | Strong artifact fit. | Less suitable if the goal is strictly an SCI graphics-method paper. |
| Computer Graphics Forum short/tool/education-oriented track | Relevant if expanded beyond a terminal-specific system. | Novelty bar is higher and likely requires broader baselines. |

## Submission Adaptation for Multimedia Tools and Applications

Manuscript structure:

1. Introduction: identify terminal line-glyph motion as a discrete
   reconstruction problem.
2. Related work: explicitly separate TAA, dirty-region rendering, terminal
   image renderers, perceptual color, and glyph topology.
3. Method: present path-indexed reconstruction first, dirty scheduling second.
4. Implementation: describe `tokitai-sign` only where it realizes the method.
5. Experiments: organize by claims C1-C4 from `docs/paper-draft.md`.
6. Results: foreground Table 4/Figure 4, then budget and metric validation.
7. Limitations: terminal-emulator variability, deterministic fixtures, no
   subjective preference data.
8. Reproducibility: artifact manifest and one-command script.

Cover-letter contribution summary:

```text
This submission studies a narrow reconstruction problem in terminal
character-cell animation: moving line primitives must be encoded as a sequence
of topology-sensitive glyphs under ANSI output budgets. The paper proposes
path-indexed temporal reconstruction, compares it with no-history and
screen-cell history baselines, and provides a reproducible Rust artifact with
dirty-budget curves, glyph reconstruction ablations, metric-validation
fixtures, and cross-terminal capture workflow.
```

## Submission Gate

Do not submit until:

- `bash scripts/reproduce_paper_artifacts.sh` exits 0;
- `artifact_manifest.csv` is regenerated and referenced by the artifact notes;
- `table_3_terminal_matrix.csv` contains at least one real screenshot-backed
  terminal/font capture for a minimum submission, and preferably three for a
  stronger cross-terminal claim;
- every major claim maps to exactly one table or figure;
- the abstract and conclusion state the narrow line-glyph reconstruction claim;
- any current quartile claim is verified again against the target journal's
  latest public indexing pages.

See `docs/submission-package.md` for the current primary/back-up venue choice,
cover-letter novelty summary, and figure captions.
