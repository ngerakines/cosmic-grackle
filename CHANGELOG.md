# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.5] - 2026-04-25

### Changed

- The MCP server and the skill are now distributed independently. The `cosmic-grackle` binary is published to GitHub Releases as per-arch macOS tarballs (Homebrew install forthcoming) and is no longer bundled into the Claude Code plugin. The marketplace plugin now ships the `macos-contacts` skill only.
- `plugin/skills/macos-contacts/SKILL.md` describes the skill's tool dependency on the externally-installed `cosmic-grackle` MCP server and tells the model to point users at the install instructions instead of running the workflow when the tools aren't present.
- `README.md` install instructions split into two sections: install the MCP server (manual download + register in your client config) and install the skill (Claude Code marketplace, optional).

### Removed

- `plugin/.mcp.json` and `plugin/hooks/` (`hooks.json`, `install-binary.sh`). The `SessionStart` binary download was specific to the Claude Code CLI plugin runtime and quietly did nothing in other Claude surfaces.
- The `cosmic-grackle-plugin.tar.gz` bundled release artifact and the `plugin` packaging job in `.github/workflows/release-binaries.yml`.

### Fixed

- Documentation no longer claims that Claude Desktop and the Claude Code CLI share `~/.claude/plugins/`. The Settings → Connectors / Skills panel in Claude Desktop ingests SKILL prose only and never started the MCP server, so users who installed the 0.1.3 plugin through that panel got the skill text but no callable tools.

## [0.1.3] - 2026-04-25

### Added

- `.claude-plugin/marketplace.json` so the plugin is installable via `/plugin marketplace add ngerakines/cosmic-grackle` and `/plugin install cosmic-grackle@cosmic-grackle` (works in both Claude Desktop and the Claude Code CLI; they share `~/.claude/plugins/`).
- `plugin/hooks/hooks.json` and `plugin/hooks/install-binary.sh` — a `SessionStart` hook that lazily downloads the universal `cosmic-grackle` binary from the matching GitHub release into `${CLAUDE_PLUGIN_ROOT}/bin/` on first session start, strips the macOS Gatekeeper quarantine, and writes a stamp file for idempotency. Subsequent sessions no-op until the version changes.

### Changed

- `plugin/.claude-plugin/plugin.json` is now the source of truth for the plugin version and is committed in lock-step with `Cargo.toml`. The release workflow validates that the committed version matches the git tag instead of rewriting the file at packaging time.
- README install instructions replaced. The previous flow (`tar -xf cosmic-grackle-plugin.tar -C ~/.claude/plugins/`) silently produced a directory that Claude's plugin registry never reads; users now install through the marketplace.

### Fixed

- Manual tarball extraction into `~/.claude/plugins/cosmic-grackle/` no longer documented as a working install path. Claude tracks plugins through `installed_plugins.json` / `known_marketplaces.json` / `cache/<marketplace>/<plugin>/<version>/`, and an orphan top-level directory is invisible to that registry.

[0.1.5]: https://github.com/ngerakines/cosmic-grackle/releases/tag/0.1.5
[0.1.3]: https://github.com/ngerakines/cosmic-grackle/releases/tag/0.1.3
