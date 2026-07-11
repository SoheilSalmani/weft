# Weft for VSCode

- **Language server**: launches `weft lsp` for `weft.toml` and
  `patches/*.json` — live `weft check` diagnostics, answer-id completion,
  hover with question/patch metadata, go-to-definition.
- **Schema validation** for patch files (bundled JSON Schema).
- **`Weft: New Project`** — pick a template, answer its required questions
  (generated from `weft describe --json`), pick a preset, scaffold.

Requires the `weft` binary on PATH (or set `weft.path`).

## Development

```sh
pnpm install && pnpm compile
# then F5 in VSCode (Run Extension)
```

Regenerate the bundled schemas with `weft schema --out editors/vscode/schemas`.
