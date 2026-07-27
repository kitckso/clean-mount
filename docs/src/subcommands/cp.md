# cp

```bash
# Copy project without node_modules, .venv, build artifacts
clean-mount cp /path/to/node-project /tmp/node-source-only
```

Internally this does: mount → `cp -a` → unmount. Your single command.
