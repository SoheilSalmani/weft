# Sources

Every factual claim in this skill traces to one of these. Vendor documentation or nothing: no comparison articles, no SEO summaries.

| Source | Publisher | URL | Accessed | Used for |
| --- | --- | --- | --- | --- |
| C2PA Specification 2.2 | C2PA / Linux Foundation | https://spec.c2pa.org/specifications/specifications/2.2/specs/C2PA_Specification.html | 2026-08-14 | Manifests, hard vs soft binding, embedding locations, recovery from a manifest repository |
| How Claude marks AI-generated content | Anthropic | https://support.claude.com/en/articles/16266773-how-claude-marks-ai-generated-content | 2026-08-14 | Text watermark and C2PA on SVG/PNG/JPG, detection status, the absence caveat |
| SynthID | Google DeepMind | https://deepmind.google/science/synthid/ | 2026-08-14 | Modalities, detector availability, robustness claims |
| Agent Skills specification | agentskills.io | https://agentskills.io/specification | 2026-08-14 | Frontmatter fields, progressive disclosure, size budgets |
| watermarks-remover @ `256d90d1` | G. Meyer, MIT | https://github.com/guillaumemeyer/watermarks-remover | 2026-08-14 | Prior art: architecture, edge cases, observed defects |

## Claims that must not drift

**Anthropic marking is EU-scoped at launch.** The documentation states that "Claude models launched **in the EU** on or after August 2, 2026 will support machine-readable marking at launch," with work continuing on existing models. Prior art recorded this as "worldwide", which the source does not say. Do not repeat the stronger claim.

**Third-party detection is not yet available.** Anthropic describes itself as "working to enable users and other third parties to detect Claude's embedded watermarks," with details in forthcoming documentation. Until that ships, Layer B presence and removal are both undeterminable.

**SynthID Detector is not generally available.** It exists as a verification portal, but is in limited testing with an early-tester waitlist, currently scoped to journalists and media professionals. It is not something this skill can invoke.

**Stripping metadata does not remove soft-bound provenance.** From the specification: soft bindings survive modifications such as re-encoding and scaling, and a manifest can be recovered by searching a repository. This is why no report may say "provenance removed".

## Stability

The two vendor pages are the volatile sources; both describe capabilities that are explicitly still rolling out. Re-check them before relying on any statement about detection availability, and update the access dates here when you do. The C2PA specification is versioned and stable. The prior-art repository is active, so its behaviour may diverge from what is recorded in the maintainer documentation.
