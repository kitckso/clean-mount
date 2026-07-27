# Use Cases

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
