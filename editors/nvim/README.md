# weft.nvim

Thin Neovim integration for [weft](../../README.md):

- attaches `weft lsp` to `weft.toml` and `patches/*.json` buffers
  (diagnostics, completion, hover, go-to-definition),
- `:WeftNew [template] [dest]` opens weft's full-screen answers wizard in a
  floating terminal — the CLI wizard *is* the TUI.

## Install

Any plugin manager; the plugin root is this directory. lazy.nvim:

```lua
{
  dir = "~/path/to/weft/editors/nvim",   -- or a git spec once published
  config = function()
    require("weft").setup({ bin = "weft" })
  end,
}
```

Requires the `weft` binary on PATH (or pass `bin`).

## Schemas without the plugin

taplo / Even Better TOML and jsonls users can get validation with no plugin
at all — see `docs/guides/editors.mdx`.
