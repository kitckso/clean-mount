# Installation

### From crates.io

```bash
cargo install clean-mount
```

### From git

```bash
git clone <repo-url>
cd clean-mount
cargo build --release
./target/release/clean-mount --help
```

### From source (local install)

After cloning, install the binary to `~/.cargo/bin/`:

```bash
cd clean-mount
cargo install --path .
clean-mount --help
```

### Docker

```bash
docker build -t clean-mount .
```

## Requirements

- **Linux:** `fuse3` runtime (`sudo apt install fuse3` or equivalent). No development headers needed to run.
- **macOS:** [macFUSE](https://osxfuse.github.io/)
