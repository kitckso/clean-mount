<div align="center">

# clean-mount

**A read-only FUSE filesystem that mirrors a directory while hiding files matched by `.gitignore`.**

[![CI](https://github.com/kitckso/clean-mount/actions/workflows/ci.yml/badge.svg)](https://github.com/kitckso/clean-mount/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/badge/crates.io-unpublished-lightgrey.svg)](https://crates.io/crates/clean-mount)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-1.88+-dea584.svg)](https://www.rust-lang.org)

Make ignored files appear nonexistent to `ls`, `find`, `zip`, `tar`, `rsync`, editors, and AI agents.

</div>

---

## The Problem

Need to send your Node.js project somewhere? `cp -r` drags along `node_modules` (500 MB). `zip -r` does the same. `rsync`, `scp`, `tar` — every tool needs its own `--exclude` rules, and you have to remember them each time.

A FUSE filesystem solves this once: **hide the junk at the filesystem layer**, and every tool just works.

## Overview

`clean-mount` mounts a **read-only** FUSE filesystem over any directory. Files and directories matched by `.gitignore` rules are invisible — they return `ENOENT` as if they never existed. Nested `.gitignore` files are respected.

```bash
# Copy a Node.js project without node_modules (one command)
clean-mount cp /path/to/project /tmp/clean-copy

# Archive a Python project without venv/__pycache__
clean-mount exec /path/to/project -- tar -czf /tmp/project.tar.gz .

# Mount interactively (auto temp dir, prints path)
clean-mount mount /path/to/project
# Mounted at: /tmp/clean-mount-XXXXX
ls /tmp/clean-mount-XXXXX
# Press Ctrl+C to unmount

# Open in file manager
clean-mount open /path/to/project
```

## Subcommands

| Subcommand | What it does |
|---|---|
| `mount SOURCE [MOUNTPOINT]` | Mount (omit mountpoint for auto temp dir + print path) |
| `open SOURCE` | Mount + open in file manager |
| `cp SOURCE DEST` | Mount, `cp -a` the filtered view to DEST, unmount |
| `exec SOURCE -- <command>` | Mount, run any command against the filtered view, unmount |

All subcommands accept the same common options (`--hide-git`, `--ignore-file`, etc.).

### `cp` — one-shot filtered copy

```bash
# Copy project without node_modules, .venv, build artifacts
clean-mount cp /path/to/node-project /tmp/clean-src
clean-mount cp /path/to/python-project /tmp/clean-src --hide-git
```

Internally this does: mount → `cp -a` → unmount. Your single command.

### `open` — browse the filtered view in your file manager

```bash
clean-mount open /path/to/project
```

Opens a temporary mount in your system file manager (nautilus, dolphin, finder, etc.).
Press Ctrl+C to unmount and close.

### `exec` — run any tool against the filtered view

```bash
# tar
clean-mount exec /path/to/project -- tar -czf /tmp/out.tar.gz .

# zip
clean-mount exec /path/to/project -- zip -r /tmp/out.zip .

# rsync
clean-mount exec /path/to/project -- rsync -avz . user@server:/deploy-path

# cp to a non-default location with extra flags
clean-mount exec /path/to/project -- cp -r . /tmp/my-copy
```

```bash
# Quick peek at what would be copied
clean-mount exec /path/to/project -- ls -la
```

The command runs **with the filtered view as its working directory** — use `.` for "everything here". Use `{MOUNT}` in arguments only when you need the absolute path explicitly:

```bash
clean-mount exec /path/to/project -- cp -r {MOUNT}/src /tmp/src-only
```

### `mount` — persistent filtered view for interactive use

```bash
mkdir -p /tmp/mirror
clean-mount mount /path/to/project /tmp/mirror
```

Then inspect, browse, or run tools against `/tmp/mirror` from another terminal.

## Features

- Mirrors an existing directory — fully transparent passthrough
- Hides files matched by `.gitignore` (supports nested `.gitignore` files)
- Read-only — safe, no accidental writes
- Symlink escape protection
- Optional `--hide-git` to hide `.git` directories
- Optional `--hide-gitignore` to hide `.gitignore` files
- Override ignore file with `--ignore-file` (e.g. `.dockerignore`)
- Configurable attribute/entry TTL (`--ttl-secs`)
- Optional `--clipboard` to copy temp mount path to clipboard
- Logging via `RUST_LOG`

> **Note:** Ignore rules are loaded at startup. If rules change, remount to reload them.

## Installation

### From crates.io

```bash
cargo install clean-mount
```

### From git

```bash
git clone https://github.com/kitckso/clean-mount.git
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

This requires the FUSE3 development headers — see [Development prerequisites](#prerequisites-for-building-from-source).

### Docker

```bash
docker build -t clean-mount .
```

See the [Docker section](#docker) for detailed usage.

## Requirements

- **Linux:** `fuse3` runtime (`sudo apt install fuse3` or equivalent). No development headers needed to run.
- **macOS:** [macFUSE](https://osxfuse.github.io/)

## Use Cases

### 📦 Copy a Node.js project without `node_modules`

```bash
clean-mount cp /path/to/node-project /tmp/node-source-only
```

Since `node_modules` is typically in `.gitignore`, it simply won't exist in the mounted view.

### 🐍 Archive a Python project without `venv` / `__pycache__`

```bash
clean-mount exec /path/to/python-project -- tar -czf /tmp/project-source.tar.gz .
```

Virtual environments, cache directories, and other gitignored files disappear automatically.

### 🚀 Rsync only source files to a server

```bash
clean-mount exec /path/to/project -- rsync -avz . user@server:/deploy-path
```

Build artifacts, dependencies, and configs (if gitignored) are excluded from the transfer.

### 🤖 Feed a clean project tree to an AI coding agent

```bash
clean-mount mount /path/to/project /tmp/clean --hide-git --hide-gitignore
# Point your AI agent at /tmp/clean
```

The agent only sees what matters — your actual source code.

## Usage

### Mount

```bash
# Auto temp dir: prints mount path, Ctrl+C to unmount
clean-mount mount /path/to/project

# Or mount at a specific directory
mkdir -p /tmp/mirror
clean-mount mount /path/to/project /tmp/mirror
```

Use another terminal to inspect the filtered view:

```bash
ls -la /tmp/mirror
cat /tmp/mirror/src/main.rs
cd /tmp/mirror && zip -r ~/filtered.zip .
```

### Unmount

| Method                | Command                       |
| --------------------- | ----------------------------- |
| Foreground process   | Press `Ctrl+C`                |
| Manual (Linux)        | `fusermount3 -u /tmp/mirror`  |
| Manual (macOS)        | `umount /tmp/mirror`          |
| Force unmount         | `fusermount3 -uz /tmp/mirror` |

### Common Options

All subcommands accept these options:

| Flag                    | Description                                                       |
| ----------------------- | ----------------------------------------------------------------- |

| `--allow-other`         | Allow other users to access the mount                             |
| `--allow-root`          | Allow root to access the mount                                    |
| `--default-permissions` | Let kernel enforce permission checks                              |
| `--ttl-secs <SECONDS>`  | Entry and attribute TTL (default: 1)                              |
| `--hide-git`            | Always hide `.git` files/directories                              |
| `--hide-gitignore`      | Always hide `.gitignore` files                                    |
| `--ignore-file <NAME>`  | Ignore file to use instead of `.gitignore` (default: `.gitignore`) |
| `--clipboard`           | Copy the auto temp mount path to clipboard                          |

### Logging

```bash
RUST_LOG=info clean-mount mount /source /mnt
RUST_LOG=clean_mount=debug clean-mount cp /source /dest
```

## Docker

Build the image:

```bash
docker build -t clean-mount .
```

Run:

```bash
docker run --rm -it \
  --device /dev/fuse \
  --cap-add SYS_ADMIN \
  --security-opt apparmor=unconfined \
  -v "$PWD/project:/source:ro" \
  clean-mount \
  mount /source /mnt
```

> **Limitation:** FUSE mounts are per-mount-namespace — they happen inside the container and are **not visible from the host**. To inspect the filtered view from another terminal:
>
> ```bash
> docker exec -it <container-id> ls /mnt
> ```
>
> The primary use of the Docker image is building and testing in CI/CD pipelines.

> **Tip:** The `sleep 1` in the use case examples waits for the mount to be ready.
> Increase this value on slower machines.

## Development

### Prerequisites (for building from source)

Building requires the FUSE3 development headers to compile the `fuser` crate:

```bash
# Debian / Ubuntu
sudo apt install libfuse3-dev pkg-config

# Fedora
sudo dnf install fuse3-devel

# macOS
brew install macfuse
```

### Running tests

```bash
cargo test
```

### Building for release

```bash
cargo build --release
```

## Contributing

Contributions are welcome! Here's how to get started:

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/my-feature`)
3. Make your changes
4. Run the tests (`cargo test && cargo build --release`)
5. Submit a pull request

Please keep changes focused and include tests when adding new functionality.

## License

This project is licensed under the [MIT License](LICENSE).
