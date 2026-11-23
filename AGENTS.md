# Agent instructions

Persistent facts about how this repository is set up. Task knowledge lives in Agent Skills; read the one that matches the work before starting it.

## Setup

- `mise install` provisions the toolchain pinned in `mise.toml`. Personal additions go in `mise.local.toml`, which is gitignored; never edit `mise.toml` for a one-machine need.

## Skills

- The skills every repository uses are each person's own, under `~/.agents/skills/` in their home directory; this repository does not carry them.
- `.agents/skills/<name>/SKILL.md` is the canonical copy of each skill this repository adds for its tracker or its stack. Codex, oh-my-pi and GitHub Copilot CLI read that directory directly.
- `.claude/skills/` is a generated symlink farm for Claude Code. Never edit or add files there; add a skill under `.agents/skills/` and run `weft update`, or link it by hand with `ln -s ../../.agents/skills/<name> .claude/skills/<name>`.

## MCP servers

- Project-scoped servers are declared in `.mcp.json` (Claude Code), `.codex/config.toml` (Codex, trusted projects only) and `.omp/mcp.json` (oh-my-pi). The three files list the same servers; change all three together.
- Only definitions are committed. Each person authorises the servers themselves: with their own account through OAuth, or with a token they export in the environment variable the server's entry names. No token or credential belongs in these files.
