# Format coverage

Every format here is handled by a container-aware parser. Nothing is processed by regex over raw bytes, and nothing is processed on the strength of its extension.

| Format | Detected by | Provenance mode removes | Always preserved |
| --- | --- | --- | --- |
| PNG | `\x89PNG\r\n\x1a\n` | `caBX` (C2PA), XMP in text chunks, AI markers in `tEXt`/`iTXt`/`zTXt` | `IHDR`, `PLTE`, `IDAT`, `IEND`, colour and rendering chunks, `eXIf` |
| JPEG | `\xff\xd8\xff` | APP11 JUMBF (C2PA), APP1 XMP, comments containing AI markers | APP1 EXIF, APP0 JFIF, APP2 ICC, quantisation and Huffman tables, scan data |
| SVG | XML with `<svg>` | `<metadata>` and RDF blocks | All drawing elements, namespace prefixes |
| HTML | `<!doctype html>` or `<html` | `<meta>` tags whose generator or AI content matches a provenance marker | Entire body, all other markup, unrelated `<meta>` |
| Markdown | Extension plus text sniff | Provenance keys in YAML frontmatter | All body content, all unrelated frontmatter keys |
| DOCX | Zip containing `word/document.xml` | Provenance values in `docProps/{core,app,custom}.xml` | `word/document.xml` and every content part |
| ODT | Zip containing `content.xml` + `meta.xml` | Provenance values in `meta.xml` | `content.xml` and every content part |
| PDF | `%PDF-` | Nothing without `exiftool`; refused rather than partially stripped | Everything |

## Pixels and payloads are never re-encoded

PNG cleaning rebuilds the chunk stream and copies `IDAT` bytes verbatim. JPEG cleaning rebuilds the segment list and copies entropy-coded scan data verbatim. No image library is involved, so a metadata clean cannot alter a single pixel. The tests assert this by comparing `IDAT` bytes before and after.

## The two modes

`provenance` is the default and targets AI provenance carriers only. **EXIF survives**, because orientation, colour profile and capture time are not AI provenance, and deleting them under a narrowly worded request is silent data loss. A photo whose orientation tag is removed displays rotated.

`all-metadata` is the broader strip and must be requested. It is a different operation and the report names it.

## Office documents

Only known property parts are touched. Arbitrary `customXml` is left alone: its meaning is application-defined and deleting it can break content controls and data bindings. The archive is rewritten entry by entry with content parts copied unchanged, and `testzip()` validates the source before anything is written.

## Refusals are the correct output

An unsupported type, a truncated container, a corrupt archive or a PDF without `exiftool` all produce a non-zero exit and an explanation. A half-processed file is worse than no file, so nothing is written on any of those paths.

## Directories

`inspect_marks.py --recursive` inspects only. There is no batch cleaner: batch mutation is where a small misclassification becomes a large amount of damage. `.git`, `node_modules`, `dist`, `build`, `.next`, `__pycache__`, `.venv` and `vendor` are skipped. Unreadable files are recorded per file and do not stop the run.
