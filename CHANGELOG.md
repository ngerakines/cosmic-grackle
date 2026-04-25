# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.3] - 2026-04-25

### Added

- `.claude-plugin/marketplace.json` so the plugin is installable via `/plugin marketplace add ngerakines/cosmic-grackle` and `/plugin install cosmic-grackle@cosmic-grackle` (works in both Claude Desktop and the Claude Code CLI; they share `~/.claude/plugins/`).
- `plugin/hooks/hooks.json` and `plugin/hooks/install-binary.sh` — a `SessionStart` hook that lazily downloads the universal `cosmic-grackle` binary from the matching GitHub release into `${CLAUDE_PLUGIN_ROOT}/bin/` on first session start, strips the macOS Gatekeeper quarantine, and writes a stamp file for idempotency. Subsequent sessions no-op until the version changes.

### Changed

- `plugin/.claude-plugin/plugin.json` is now the source of truth for the plugin version and is committed in lock-step with `Cargo.toml`. The release workflow validates that the committed version matches the git tag instead of rewriting the file at packaging time.
- README install instructions replaced. The previous flow (`tar -xf cosmic-grackle-plugin.tar -C ~/.claude/plugins/`) silently produced a directory that Claude's plugin registry never reads; users now install through the marketplace.

### Fixed

- Manual tarball extraction into `~/.claude/plugins/cosmic-grackle/` no longer documented as a working install path. Claude tracks plugins through `installed_plugins.json` / `known_marketplaces.json` / `cache/<marketplace>/<plugin>/<version>/`, and an orphan top-level directory is invisible to that registry.

[0.1.3]: https://github.com/ngerakines/cosmic-grackle/releases/tag/0.1.3
