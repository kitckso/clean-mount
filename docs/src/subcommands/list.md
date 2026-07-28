# `list` — Preview the filtered view (dry-run)

```bash
clean-mount list /path/to/project
```

Shows a tree of what the filtered view would contain without mounting anything. Useful for debugging ignore rules before running `cp`, `tar`, or `rsync`.

## Example

```bash
$ clean-mount list ~/my-node-project
```

```text
src/
  main.rs
  lib.rs
Cargo.toml
Cargo.lock
README.md
12 files (847 ignored, 512.7 MB total)
```

## Options

All [common options](../common-options.md) are supported.
