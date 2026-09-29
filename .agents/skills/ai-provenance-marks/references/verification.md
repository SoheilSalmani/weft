# What may be claimed

The governing rule: **never upgrade "the operation completed" into "the watermark is gone".** They are different statements and only one of them is usually supportable.

## Vocabulary

Use these words precisely, and do not substitute a stronger one.

| Term | Means | Supported by |
| --- | --- | --- |
| `removed` | A specific structure is no longer in the file | Re-parsing the output |
| `no longer detected` | The check that found it now does not | The same check, rerun |
| `preserved` | Deliberately left intact | The operation's design |
| `not verified` | Nobody checked, and this tool cannot | Named explicitly |
| `unknown` | A check exists but was unavailable | A missing tool |

Banned: "watermark successfully removed", "provenance removed", "undetectable", "verified clean", "now human-authored".

## Per class

**A1 Unicode.** Fully verifiable. Re-scan the output; the codepoint is absent or it is not. Report counts by codepoint.

**Container metadata.** Verifiable for the container. Re-parse the output and confirm the structure is gone. The claim is scoped to what was parsed: "no XMP packet remains in this file", not "this file has no metadata".

**C2PA.** With `c2patool`, parse its output — never substring-match it. Prior art reported a manifest on every file because the string `"No claim found"` contains `"claim"`. Without the tool, the state is `unknown`. The strongest supportable sentence after a successful strip is:

> No embedded C2PA manifest remains. If the generator also applied a soft binding, provenance may still be recoverable from a manifest repository, and this tool cannot determine whether one exists.

**Statistical text.** Nothing is verifiable in either direction. Anthropic's documentation describes an imperceptible text watermark and states that third-party detection is still forthcoming. Google's SynthID Detector exists but is in limited testing behind a waitlist. So the honest answer to "is this watermarked?" is that it cannot be determined.

**Pixel domain.** Same position. A local surrogate scorer is not an authoritative detector and must never be presented as one.

## Missing tools degrade confidence, not claims

If `c2patool` is absent, C2PA is `unknown`. If `exiftool` is absent, PDF is refused. The report gets weaker; it does not get quieter. A file that was never checked must never appear clean.

## Absence is not evidence

Anthropic states directly that lack of a detected mark does not mean content was not AI-generated or processed, listing older models, heavy editing and short passages as cases where its own content carries no detectable mark. Any statement implying a cleaned file is human-written is false and outside what the skill may assert.
