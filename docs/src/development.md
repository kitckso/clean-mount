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

## Running tests

```bash
cargo test
```

## Building for release

```bash
cargo build --release
```
