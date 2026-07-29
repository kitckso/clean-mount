# Development

## Prerequisites (for building from source)

Building requires the FUSE3 development headers to compile the `fuser` crate:

```bash
# Debian / Ubuntu
sudo apt install libfuse3-dev pkg-config

# Fedora
sudo dnf install fuse3-devel

# macOS
brew install macfuse
```

## Pre-commit hooks (prek)

This repo uses [prek](https://github.com/j178/prek) to run `cargo fmt` and `cargo clippy` before each commit.

```bash
# Install prek (one time)
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/j178/prek/releases/download/v0.4.11/prek-installer.sh | sh

# Install the git hook (per clone)
prek install
```

## Running tests

```bash
cargo test
```

## Building for release

```bash
cargo build --release
```
