# complete

```bash
# Add to ~/.bashrc, ~/.zshrc, ~/.config/fish/config.fish, etc.
eval "$(clean-mount complete)"
```

Generates a shell completion script so you can tab-complete subcommands, options, and paths.

### Auto-install

```bash
clean-mount complete --install
```

Detects your shell from `$SHELL` and appends `eval "$(clean-mount complete)"` to the appropriate rc file (`.bashrc`, `.zshrc`, `.config/fish/config.fish`, `.config/elvish/rc.elv`). Pass a shell explicitly to install for a different shell:

```bash
clean-mount complete --install zsh
```

### Manual install

Auto-detects your shell from `$SHELL`. Pass the shell name explicitly for other shells:

```bash
# bash
clean-mount complete bash > ~/.local/share/bash-completion/completions/clean-mount

# zsh (ensure ~/.zsh/completions is in your fpath)
mkdir -p ~/.zsh/completions
clean-mount complete zsh > ~/.zsh/completions/_clean-mount

# fish
clean-mount complete fish > ~/.config/fish/completions/clean-mount.fish
```
