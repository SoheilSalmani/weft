# Agent instructions

Persistent facts about how this repository is set up. Task knowledge lives in the skills under `.agents/skills/`; read the one that matches the work before starting it.

## Setup

- `mise install` provisions the toolchain pinned in `mise.toml`. Personal additions go in `mise.local.toml`, which is gitignored; never edit `mise.toml` for a one-machine need.

## Skills

- `.agents/skills/<name>/SKILL.md` is the canonical copy of every skill. Codex and oh-my-pi read that directory directly.
- `.claude/skills/` is a generated symlink farm for Claude Code. Never edit or add files there; add a skill under `.agents/skills/` and run `weft update`, or link it by hand with `ln -s ../../.agents/skills/<name> .claude/skills/<name>`.

## MCP servers

- Project-scoped servers are declared in `.mcp.json` (Claude Code), `.codex/config.toml` (Codex, trusted projects only) and `.omp/mcp.json` (oh-my-pi). The three files list the same servers; change all three together.
- Only definitions are committed. Each person authorises remote servers with their own account (OAuth), so no token, header or credential belongs in these files.
