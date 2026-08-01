<div align="center">

# clean-mount

**A read-only FUSE filesystem that mirrors a directory while hiding files matched by `.gitignore`.**

[![CI](https://github.com/kitckso/clean-mount/actions/workflows/ci.yml/badge.svg)](https://github.com/kitckso/clean-mount/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/clean-mount)](https://crates.io/crates/clean-mount)
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
| `mount SOURCE [MOUNTPOINT]` | Mount (omit mountpoint for auto temp dir + print path). Use `--daemon` to run in background. |
| `status` | List active daemon mounts (PID, source, mountpoint, uptime) |
| `stop --pid <PID>` / `stop <MOUNTPOINT>` | Unmount a running daemon mount by PID or mountpoint |
| `open SOURCE` | Mount + open in file manager |
| `cp SOURCE DEST` | Mount, `cp -a` the filtered view to DEST, unmount |
| `list SOURCE` | Preview the filtered view without mounting (flat listing, no summary) |
| `tar SOURCE OUTPUT` | Mount, create tarball of the filtered view, unmount (compression from suffix) |
| `zip SOURCE OUTPUT` | Mount, create `.zip`  of the filtered view, unmount |
| `exec SOURCE -- <command>` | Mount, run any command against the filtered view, unmount |
| `complete [SHELL]` | Generate shell completion script (bash, zsh, fish, elvish, powershell). Use `--install` to auto-add the eval line to your shell rc file. |

`tar`, `zip`, `cp`, `mount`, `open`, `exec`, and `list` accept the same common options (`--hide-git`, `--ignore-file`, etc.). `complete` does not need them.

### `complete` — shell tab completion

```bash
# Add to ~/.bashrc, ~/.zshrc, etc.
eval "$(clean-mount complete)"
```

Or let clean-mount add the line to your rc file automatically:

```bash
clean-mount complete --install
```

Pass a shell to install for a different shell than `$SHELL`:

```bash
clean-mount complete --install zsh
```

Auto-detects your shell from `$SHELL`. Pass a shell name explicitly for other shells:

```bash
# bash
clean-mount complete bash > ~/.local/share/bash-completion/completions/clean-mount

# zsh (ensure ~/.zsh/completions is in your fpath)
mkdir -p ~/.zsh/completions
clean-mount complete zsh > ~/.zsh/completions/_clean-mount

# fish
clean-mount complete fish > ~/.config/fish/completions/clean-mount.fish
```


### `cp` — one-shot filtered copy

```bash
# Copy project without node_modules, .venv, build artifacts
clean-mount cp /path/to/node-project /tmp/clean-src
clean-mount cp /path/to/python-project /tmp/clean-src --hide-git
```

Internally this does: mount → `cp -a` → unmount. Your single command.

### `list` — preview the filtered view without mounting (dry-run)

```bash
clean-mount list /path/to/project
```

Shows what the filtered view would contain without mounting anything.
Useful for debugging ignore rules before running `cp`, `tar`, or `rsync`.

```bash
# Flat top-level listing (default)
clean-mount list /path/to/project

# Full recursive tree
clean-mount list /path/to/project --tree

# Show summary statistics
clean-mount list /path/to/project --summary

# Check if specific ignore rules work as expected
clean-mount list /path/to/project --hide-git --hide-gitignore

# Use a different ignore file (e.g. .dockerignore)
clean-mount list /path/to/project --ignore-file .dockerignore

# Show everything, ignoring any ignore rules
clean-mount list /path/to/project --no-ignore

# Hide extra paths on top of the ignore file (overrides it)
clean-mount list /path/to/project --exclude '*.min.js' --exclude build/

# Keep a gitignored file visible
clean-mount list /path/to/project --include keep.env

# Ad-hoc filtering without any ignore file
clean-mount list /path/to/project --no-ignore --exclude '*.log' --exclude .venv
```

`--exclude` and `--include` accept gitignore-style patterns and can be repeated. Precedence (highest to lowest): `--hide-git`/`--hide-gitignore`, `--exclude`, `--include`, then the ignore file. `--no-ignore` disables only the ignore-file rules, so it combines naturally with `--exclude` (or `--include`) for one-off filtering when no `.gitignore` exists.

Example output:

```text
$ clean-mount list /path/to/project
src/
Cargo.toml
Cargo.lock
README.md

$ clean-mount list /path/to/project --tree --summary
src/
  main.rs
  lib.rs
Cargo.toml
Cargo.lock
README.md
12 files (847 ignored, 512.7 MB total)
```

### `open` — browse the filtered view in your file manager

```bash
clean-mount open /path/to/project
```

Opens a temporary mount in your system file manager (nautilus, dolphin, finder, etc.).
Press Ctrl+C to unmount and close.

### `tar` — one-shot filtered tarball

```bash
# gzip
clean-mount tar /path/to/project /tmp/project.tgz

# xz
clean-mount tar /path/to/project /tmp/project.tar.xz

# bzip2
clean-mount tar /path/to/project /tmp/project.tar.bz2

# no compression
clean-mount tar /path/to/project /tmp/project.tar
```

Compression auto-detected from suffix (`.tar` = none, `.tar.gz`/`.tgz` = gzip, `.tar.xz`/`.txz` = xz, `.tar.bz2`/`.tbz2`/`.tbz` = bzip2, `.tar.zst`/`.tzst` = zstd). Internally: mount → `tar -acf` → unmount.

### `zip` — one-shot filtered zip archive

```bash
clean-mount zip /path/to/project /tmp/project.zip
```

Internally: mount → `zip -r` → unmount.

### `exec` — run any tool against the filtered view

```bash
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

Use `--daemon` to run the mount in the background (requires an explicit mountpoint):

```bash
clean-mount mount /path/to/project /tmp/mirror --daemon
# Returns immediately, prints PID
```

This behaves like a classic daemon — the process forks, detaches from the terminal, and
keeps the mount alive. The mount also auto-exits when unmounted externally.

### `status` — list active daemon mounts

```bash
clean-mount status
# PID   SOURCE                    MOUNTPOINT                               UPTIME
# 12345 /home/user/project       /tmp/mirror                               2h 15m
```

Shows all running daemon mounts registered with a PID file. Dead PIDs are filtered out.

### `stop` — unmount a daemon mount

```bash
# By PID
clean-mount stop --pid 12345

# By mountpoint
clean-mount stop /tmp/mirror
```

Internally runs `fusermount3 -u` (or `umount` as fallback) against the resolved mountpoint.

## Features

- Mirrors an existing directory — fully transparent passthrough
- Hides files matched by `.gitignore` (supports nested `.gitignore` files)
- Read-only — safe, no accidental writes
- Symlink escape protection
- Optional `--hide-git` to hide `.git` directories
- Optional `--hide-gitignore` to hide `.gitignore` files
- Override ignore file with `--ignore-file` (e.g. `.dockerignore`); errors if the file is not found
- `--no-ignore` to disable ignore-file processing entirely (show all files); pair it with `--exclude` to filter ad-hoc without any ignore file
- `--exclude <PATTERN>` to hide extra paths on top of (or instead of) the ignore file
- `--include <PATTERN>` to keep paths visible even when the ignore file hides them
- Configurable attribute/entry TTL (`--ttl-secs`)
- Optional `--clipboard` to copy temp mount path to clipboard
- `--daemon` mode for background mounts with PID file tracking
- `status` and `stop` subcommands to manage daemon mounts
- Auto-exit when mount is unmounted externally
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
clean-mount tar /path/to/python-project /tmp/project-source.tar.gz
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

### 🐳 Inspect what goes into a Docker build

```bash
clean-mount list /path/to/project --ignore-file .dockerignore --tree --summary
```

See exactly which files and how much data would be sent to the Docker daemon. Avoid bloated images by verifying your `.dockerignore` rules — no build needed.

## Usage

### Mount

```bash
# Auto temp dir: prints mount path, Ctrl+C to unmount
clean-mount mount /path/to/project

# Or mount at a specific directory
mkdir -p /tmp/mirror
clean-mount mount /path/to/project /tmp/mirror

# Daemon mode: runs in background, prints PID
clean-mount mount /path/to/project /tmp/mirror --daemon
```

Use another terminal to inspect the filtered view:

```bash
ls -la /tmp/mirror
cat /tmp/mirror/src/main.rs
cd /tmp/mirror && zip -r ~/filtered.zip .
```

### Unmount

| Method                | Command                             |
| --------------------- | ----------------------------------- |
| Foreground process   | Press `Ctrl+C`                      |
| Daemon mount         | `clean-mount stop --pid <PID>`      |
| Daemon mount         | `clean-mount stop /tmp/mirror`      |
| Manual (Linux)        | `fusermount3 -u /tmp/mirror`        |
| Manual (macOS)        | `umount /tmp/mirror`                |
| Force unmount         | `fusermount3 -uz /tmp/mirror`       |

### Common Options

All subcommands accept these options. `list` also accepts `--tree`/`-t` and `--summary`/`-s`:

| Flag                    | Description                                                       |
| ----------------------- | ----------------------------------------------------------------- |
| `--allow-other`         | Allow other users to access the mount                             |
| `--allow-root`          | Allow root to access the mount                                    |
| `--default-permissions` | Let kernel enforce permission checks                              |
| `--ttl-secs <SECONDS>`  | Entry and attribute TTL (default: 1)                              |
| `--hide-git`            | Always hide `.git` files/directories                              |
| `--hide-gitignore`      | Always hide `.gitignore` files                                    |
| `--ignore-file <NAME>`  | Ignore file to use instead of `.gitignore` (default: `.gitignore`); errors if not found |
| `--no-ignore`           | Disable ignore-file processing entirely (show all files); pair with `--exclude` to filter ad-hoc without an ignore file |
| `--exclude <PATTERN>`   | Extra gitignore-style pattern(s) to hide; overrides the ignore file and `--include`. Repeatable |
| `--include <PATTERN>`   | Gitignore-style pattern(s) to keep visible even if the ignore file hides them; overridden by `--exclude`. Repeatable |
| `--clipboard`           | Copy the auto temp mount path to clipboard                          |
| `--tree` / `-t`         | Show recursive directory tree (list only)                           |
| `--summary` / `-s`      | Show file/ignored/size summary (list only)                          |

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
