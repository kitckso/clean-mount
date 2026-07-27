# Rsync

```bash
clean-mount exec /path/to/project -- rsync -avz . user@server:/deploy-path
```

Build artifacts, dependencies, and configs (if gitignored) are excluded from the transfer.
