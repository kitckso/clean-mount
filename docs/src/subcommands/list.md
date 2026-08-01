# `list` — Preview the filtered view (dry-run)

```bash
clean-mount list /path/to/project [--tree] [--summary]
```

Shows what the filtered view would contain without mounting anything. Useful for debugging ignore rules before running `cp`, `tar`, or `rsync`.

By default, `list` shows a flat top-level listing (like `ls`) with no summary. Use `--tree` for the full recursive tree, and `--summary` for file/ignored/size statistics.

## Examples

```bash
# Flat top-level listing (default, fast)
$ clean-mount list ~/my-node-project
```

```text
src/
node_modules/
package.json
```

```bash
# Full recursive tree with summary stats
$ clean-mount list ~/my-node-project --tree --summary
```

```text
src/
  index.js
  lib/
    utils.js
    parser.js
package.json
12 files (847 ignored, 512.7 MB total)
```

```bash
# Show everything, ignoring any ignore rules
clean-mount list ~/my-node-project --no-ignore

# Hide extra paths on top of the ignore file (overrides it)
clean-mount list ~/my-node-project --exclude '*.min.js' --exclude build/

# Keep a gitignored file visible
clean-mount list ~/my-node-project --include keep.env

# Ad-hoc filtering without any ignore file
clean-mount list ~/my-node-project --no-ignore --exclude '*.log' --exclude .venv
```

## Options

| Flag | Description |
|------|-------------|
| `--tree` / `-t` | Show full recursive directory tree |
| `--summary` / `-s` | Show file/ignored/size summary |

All [common options](../common-options.md) are also supported. `--exclude`/`--include` take gitignore-style patterns (repeatable) and override the ignore file; pair `--no-ignore` with `--exclude` for ad-hoc filtering without any ignore file.
