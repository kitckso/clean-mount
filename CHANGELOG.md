# Changelog

## [0.1.3] - 2026-07-30

### Added

- `complete [SHELL]` — generate shell completion script for bash, zsh, fish, elvish, powershell.
- `complete --install` — auto-install completions by appending `eval "$(clean-mount complete)"` to the shell rc file.

## [0.1.2] - 2026-07-29

### Added

- `list --tree` / `-t` — show full recursive directory tree (default: flat list).
- `list --summary` / `-s` — show summary statistics (default: hidden).
- `tar SOURCE OUTPUT` — create a tarball of the filtered view (compression auto-detected from suffix e.g. `.tar.gz`, `.tgz`, `.tar.xz`, `.tar.bz2`, `.tar.zst`, `.tar`).
- `zip SOURCE OUTPUT` — create a zip archive of the filtered view.

### Changed

- `list` default changed to flat top-level listing without summary for performance on large trees.
- Summary stats now use a separate full-tree walk (unchanged accuracy).

## [0.1.1] - 2026-07-28

### Added

- `list` subcommand — dry-run / preview mode that shows the filtered view without mounting, including a tree of visible files and summary stats.
- CHANGELOG.md.

### Changed

- Deploy-docs workflow: filter to docs-only paths, use `peaceiris/actions-mdbook@v2`.
- Release workflow: `ubuntu-latest` with `ubuntu:20.04` container for backward-compatible binaries; install Rust via `actions-rust-lang/setup-rust-toolchain`.
- CI workflow: bump `actions/checkout` to v5, `actions/cache` to v5 (Node.js 24 runtime).
- Remove stale `sleep 1` tip from README.

## [0.1.0] - 2026-07-27

### Added

- Initial release of `clean-mount` — a read-only FUSE filesystem that mirrors a directory while hiding files matched by `.gitignore` rules.
- Subcommands: `mount`, `open`, `cp`, `exec`
- Nested `.gitignore` support
- Options: `--hide-git`, `--hide-gitignore`, `--ignore-file`, `--ttl-secs`, `--allow-other`, `--allow-root`, `--default-permissions`, `--clipboard`
- Symlink escape protection
- Docker image for CI/CD pipelines
- CI workflow with checks on multiple platforms (ubuntu-latest, macOS)
- Release workflow publishing to crates.io and attaching release binaries
- mdBook documentation with GitHub Pages deployment
- Devcontainer configuration for FUSE development in Codespaces
