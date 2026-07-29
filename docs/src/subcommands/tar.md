# tar

Supported suffixes:

| Suffix | Compression |
|---|---|
| `.tar` | none |
| `.tar.gz` / `.tgz` | gzip |
| `.tar.xz` / `.txz` | xz |
| `.tar.bz2` / `.tbz2` / `.tbz` | bzip2 |
| `.tar.zst` / `.tzst` | zstd |

```bash
clean-mount tar /path/to/project /tmp/project.tgz
clean-mount tar /path/to/project /tmp/project.tar.xz
```

Internally: mount → `tar -acf` → unmount.
