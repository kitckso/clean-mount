# Changelog

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
