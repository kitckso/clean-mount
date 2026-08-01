use anyhow::{Context, Result};
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use ignore::WalkBuilder;
use std::ffi::OsStr;
use std::fs::FileType;
use std::path::{Path, PathBuf};

/// A single ignore rule set scoped to a specific directory.
struct ScopedRule {
    /// The directory this rule applies to (relative to root).
    /// Empty path means the root directory.
    scope: PathBuf,
    /// Gitignore matcher whose root is set to the scope directory,
    /// so patterns are interpreted relative to the scope.
    gitignore: Gitignore,
}

pub struct IgnoreMatcher {
    rules: Vec<ScopedRule>,
    found_ignore_file: bool,
    /// Whether an entry named after the ignore file was seen, regardless of
    /// file type (e.g. a directory). Used to report a more precise error.
    saw_ignore_file_name: bool,
    hide_git: bool,
    hide_gitignore: bool,
    /// CLI `--exclude` patterns: always hide, override everything else.
    exclude: Gitignore,
    /// CLI `--include` patterns: keep visible despite the ignore file.
    include: Gitignore,
}

/// Ignore-related settings shared by the filesystem and the `list` preview.
pub struct IgnoreConfig<'a> {
    pub hide_git: bool,
    pub hide_gitignore: bool,
    pub ignore_file_name: Option<&'a OsStr>,
    pub require_ignore_file: bool,
    pub exclude: &'a [String],
    pub include: &'a [String],
}

impl IgnoreMatcher {
    pub fn new(root: &Path, config: &IgnoreConfig) -> Result<Self> {
        let mut rules: Vec<ScopedRule> = Vec::new();
        let mut found_ignore_file = false;
        let mut saw_ignore_file_name = false;

        let exclude = build_override_matcher(root, config.exclude, "--exclude")?;
        let include = build_override_matcher(root, config.include, "--include")?;

        let Some(ignore_file_name) = config.ignore_file_name else {
            return Ok(Self {
                rules,
                found_ignore_file,
                saw_ignore_file_name: false,
                hide_git: config.hide_git,
                hide_gitignore: config.hide_gitignore,
                exclude,
                include,
            });
        };

        let walker = WalkBuilder::new(root)
            .standard_filters(false)
            .hidden(false)
            .git_ignore(false)
            .git_global(false)
            .git_exclude(false)
            .follow_links(false)
            .build();

        for entry in walker {
            let entry = match entry {
                Ok(entry) => entry,
                Err(err) => {
                    tracing::warn!(
                        error = %err,
                        "skipping unreadable path while loading ignore files"
                    );
                    continue;
                }
            };

            if entry.file_name() == ignore_file_name {
                saw_ignore_file_name = true;
            }

            if entry.file_name() == ignore_file_name
                && entry.file_type().is_some_and(|ft| ft.is_file())
            {
                found_ignore_file = true;

                // The scope is the parent directory of the ignore file, relative to root.
                let Ok(rel_path) = entry.path().strip_prefix(root) else {
                    continue;
                };
                let scope = rel_path
                    .parent()
                    .map_or_else(PathBuf::new, Path::to_path_buf);

                // Build a separate Gitignore with its root set to the scope directory.
                // This ensures patterns are interpreted relative to the scope,
                // not the project root.
                let scope_root = root.join(&scope);
                let mut scope_builder = GitignoreBuilder::new(&scope_root);
                if let Some(err) = scope_builder.add(entry.path()) {
                    tracing::warn!(
                        error = %err,
                        path = %entry.path().display(),
                        "failed to add ignore file"
                    );
                    continue;
                }
                if let Ok(gitignore) = scope_builder.build() {
                    rules.push(ScopedRule { scope, gitignore });
                }
            }
        }

        Ok(Self {
            rules,
            found_ignore_file,
            saw_ignore_file_name,
            hide_git: config.hide_git,
            hide_gitignore: config.hide_gitignore,
            exclude,
            include,
        })
    }

    /// Errors if an explicitly requested ignore file is missing, and warns if
    /// the default ignore file is missing. Does nothing when ignore files are
    /// disabled (`ignore_file_name` is `None`).
    pub fn ensure_ignore_file(&self, config: &IgnoreConfig, source: &Path) -> anyhow::Result<()> {
        let Some(name) = config.ignore_file_name else {
            return Ok(());
        };

        if config.require_ignore_file {
            if !self.found_ignore_file {
                if self.saw_ignore_file_name {
                    anyhow::bail!(
                        "ignore file {name:?} in {} is not a regular file",
                        source.display()
                    );
                }
                anyhow::bail!("ignore file {name:?} not found in {}", source.display());
            }
        } else if !self.found_ignore_file && config.exclude.is_empty() && config.include.is_empty()
        {
            tracing::warn!(
                "no {name:?} found in {}; nothing will be filtered",
                source.display()
            );
        }

        Ok(())
    }

    /// Returns true if the path or any of its ancestors (up to but not
    /// including the root) is matched as ignored by `matcher`.
    fn ancestor_matched(
        &self,
        matcher: &Gitignore,
        rel: &Path,
        file_type: Option<FileType>,
    ) -> bool {
        for ancestor in rel.ancestors() {
            if ancestor.as_os_str().is_empty() {
                continue;
            }

            let is_dir = if ancestor == rel {
                file_type.is_some_and(|ft| ft.is_dir())
            } else {
                true
            };

            if matcher.matched(ancestor, is_dir).is_ignore() {
                return true;
            }
        }
        false
    }

    pub fn is_ignored(&self, rel: &Path, file_type: Option<FileType>) -> bool {
        if rel.as_os_str().is_empty() {
            return false;
        }

        if let Some(name) = rel.file_name() {
            if self.hide_git && name == OsStr::new(".git") {
                return true;
            }

            if self.hide_gitignore && name == OsStr::new(".gitignore") {
                return true;
            }
        }

        // --exclude wins over everything: hides the path and everything below it.
        if self.ancestor_matched(&self.exclude, rel, file_type) {
            return true;
        }

        // --include overrides the ignore file: keeps the path visible.
        if self.ancestor_matched(&self.include, rel, file_type) {
            return false;
        }

        // Check every ancestor so an ignored directory hides all children.
        for ancestor in rel.ancestors() {
            if ancestor.as_os_str().is_empty() {
                continue;
            }

            let is_dir = if ancestor == rel {
                file_type.is_some_and(|ft| ft.is_dir())
            } else {
                true
            };

            for rule in &self.rules {
                // Only apply this rule if the path is within the rule's scope.
                // Strip the scope prefix so the remaining path is relative to
                // the .gitignore file's own directory.
                if let Ok(stripped) = ancestor.strip_prefix(&rule.scope) {
                    if !stripped.as_os_str().is_empty()
                        && rule.gitignore.matched(stripped, is_dir).is_ignore()
                    {
                        return true;
                    }
                }
            }
        }

        false
    }
}

fn build_override_matcher(root: &Path, patterns: &[String], flag: &str) -> Result<Gitignore> {
    let mut builder = GitignoreBuilder::new(root);
    for pattern in patterns {
        builder
            .add_line(None, pattern)
            .with_context(|| format!("invalid {flag} pattern {pattern:?}"))?;
    }
    builder
        .build()
        .with_context(|| format!("failed to build {flag} matcher"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn ft(path: &Path) -> Option<FileType> {
        fs::symlink_metadata(path).ok().map(|m| m.file_type())
    }

    fn matcher(
        root: &Path,
        hide_git: bool,
        hide_gitignore: bool,
        ignore_file: Option<&OsStr>,
        exclude: &[String],
        include: &[String],
    ) -> IgnoreMatcher {
        IgnoreMatcher::new(
            root,
            &IgnoreConfig {
                hide_git,
                hide_gitignore,
                ignore_file_name: ignore_file,
                require_ignore_file: false,
                exclude,
                include,
            },
        )
        .unwrap()
    }

    #[test]
    fn respects_root_gitignore() {
        let dir = tempdir().unwrap();

        fs::write(dir.path().join(".gitignore"), "secret.txt\ntarget/\n").unwrap();
        fs::write(dir.path().join("secret.txt"), "secret").unwrap();
        fs::write(dir.path().join("keep.txt"), "keep").unwrap();

        fs::create_dir(dir.path().join("target")).unwrap();
        fs::write(dir.path().join("target").join("a.txt"), "artifact").unwrap();

        let matcher = matcher(
            dir.path(),
            false,
            false,
            Some(OsStr::new(".gitignore")),
            &[],
            &[],
        );

        assert!(matcher.is_ignored(Path::new("secret.txt"), ft(&dir.path().join("secret.txt"))));

        assert!(!matcher.is_ignored(Path::new("keep.txt"), ft(&dir.path().join("keep.txt"))));

        assert!(matcher.is_ignored(Path::new("target"), ft(&dir.path().join("target"))));

        assert!(matcher.is_ignored(
            Path::new("target/a.txt"),
            ft(&dir.path().join("target").join("a.txt"))
        ));
    }

    #[test]
    fn respects_nested_gitignore() {
        let dir = tempdir().unwrap();

        fs::create_dir(dir.path().join("src")).unwrap();
        fs::write(dir.path().join("src").join(".gitignore"), "*.log\n").unwrap();
        fs::write(dir.path().join("src").join("debug.log"), "log").unwrap();
        fs::write(dir.path().join("src").join("main.rs"), "fn main() {}").unwrap();

        let matcher = matcher(
            dir.path(),
            false,
            false,
            Some(OsStr::new(".gitignore")),
            &[],
            &[],
        );

        assert!(matcher.is_ignored(
            Path::new("src/debug.log"),
            ft(&dir.path().join("src").join("debug.log"))
        ));

        assert!(!matcher.is_ignored(
            Path::new("src/main.rs"),
            ft(&dir.path().join("src").join("main.rs"))
        ));
    }

    #[test]
    fn hides_git_and_gitignore_when_requested() {
        let dir = tempdir().unwrap();

        fs::create_dir(dir.path().join(".git")).unwrap();
        fs::write(dir.path().join(".gitignore"), "secret\n").unwrap();
        fs::write(dir.path().join("file.txt"), "x").unwrap();

        let matcher = matcher(
            dir.path(),
            true,
            true,
            Some(OsStr::new(".gitignore")),
            &[],
            &[],
        );

        assert!(matcher.is_ignored(Path::new(".git"), ft(&dir.path().join(".git"))));
        assert!(matcher.is_ignored(Path::new(".gitignore"), ft(&dir.path().join(".gitignore"))));
        assert!(!matcher.is_ignored(Path::new("file.txt"), ft(&dir.path().join("file.txt"))));
    }

    #[test]
    /// Nested .gitignore patterns must NOT leak outside their directory scope.
    fn nested_gitignore_does_not_leak() {
        let dir = tempdir().unwrap();

        // Root .gitignore: only hides .log files
        fs::write(dir.path().join(".gitignore"), "*.log\n").unwrap();

        // Nested .gitignore: hides EVERYTHING inside subdir/
        fs::create_dir(dir.path().join("subdir")).unwrap();
        fs::write(dir.path().join("subdir").join(".gitignore"), "*\n").unwrap();

        // Files OUTSIDE the nested scope
        fs::write(dir.path().join("README.md"), "readme").unwrap();
        fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();

        // Files INSIDE the nested scope
        fs::write(dir.path().join("subdir").join("keep.txt"), "keep").unwrap();
        fs::write(dir.path().join("subdir").join("secret.key"), "secret").unwrap();

        let matcher = matcher(
            dir.path(),
            false,
            false,
            Some(OsStr::new(".gitignore")),
            &[],
            &[],
        );

        // These should NOT be ignored (they're outside subdir/'s scope)
        assert!(
            !matcher.is_ignored(Path::new("README.md"), ft(&dir.path().join("README.md"))),
            "README.md outside subdir/ should NOT be ignored"
        );
        assert!(
            !matcher.is_ignored(Path::new("main.rs"), ft(&dir.path().join("main.rs"))),
            "main.rs outside subdir/ should NOT be ignored"
        );

        // These SHOULD be ignored (they're inside subdir/ where * matches everything)
        assert!(
            matcher.is_ignored(
                Path::new("subdir/keep.txt"),
                ft(&dir.path().join("subdir/keep.txt"))
            ),
            "subdir/keep.txt should be ignored by subdir/.gitignore's * pattern"
        );
        assert!(
            matcher.is_ignored(
                Path::new("subdir/secret.key"),
                ft(&dir.path().join("subdir/secret.key"))
            ),
            "subdir/secret.key should be ignored by subdir/.gitignore's * pattern"
        );
    }

    #[test]
    /// .gitignore files inside already-gitignored directories should not leak patterns.
    fn ignores_inside_ignored_dirs_are_skipped() {
        let dir = tempdir().unwrap();

        // Root .gitignore ignores .venv/
        fs::write(dir.path().join(".gitignore"), ".venv/\n").unwrap();

        // Nested .gitignore inside ignored dir with wildcard
        fs::create_dir_all(dir.path().join(".venv")).unwrap();
        fs::write(dir.path().join(".venv").join(".gitignore"), "*\n").unwrap();

        // A file at root level
        fs::write(dir.path().join("app.py"), "print('hello')").unwrap();

        let matcher = matcher(
            dir.path(),
            false,
            false,
            Some(OsStr::new(".gitignore")),
            &[],
            &[],
        );

        // .venv/ itself is ignored by root .gitignore
        assert!(matcher.is_ignored(Path::new(".venv"), ft(&dir.path().join(".venv"))));

        // app.py should NOT be ignored (the * from .venv/.gitignore must not leak)
        assert!(
            !matcher.is_ignored(Path::new("app.py"), ft(&dir.path().join("app.py"))),
            "app.py should NOT be ignored by .venv/.gitignore's * pattern"
        );
    }

    #[test]
    fn no_ignore_file_disables_rules() {
        let dir = tempdir().unwrap();

        fs::write(dir.path().join(".gitignore"), "secret.txt\n").unwrap();
        fs::write(dir.path().join("secret.txt"), "secret").unwrap();

        let matcher = matcher(dir.path(), false, false, None, &[], &[]);

        assert!(!matcher.found_ignore_file);
        assert!(!matcher.is_ignored(Path::new("secret.txt"), ft(&dir.path().join("secret.txt"))));
    }

    #[test]
    fn exclude_overrides_gitignore_whitelist() {
        let dir = tempdir().unwrap();

        fs::write(dir.path().join(".gitignore"), "!keep.txt\n").unwrap();
        fs::write(dir.path().join("keep.txt"), "keep").unwrap();
        fs::write(dir.path().join("other.txt"), "other").unwrap();

        let matcher = matcher(
            dir.path(),
            false,
            false,
            Some(OsStr::new(".gitignore")),
            &["keep.txt".to_string()],
            &[],
        );

        assert!(matcher.is_ignored(Path::new("keep.txt"), ft(&dir.path().join("keep.txt"))));
        assert!(!matcher.is_ignored(Path::new("other.txt"), ft(&dir.path().join("other.txt"))));
    }

    #[test]
    fn exclude_hides_directory_and_children() {
        let dir = tempdir().unwrap();

        fs::create_dir(dir.path().join("build")).unwrap();
        fs::write(dir.path().join("build").join("a.o"), "obj").unwrap();

        let matcher = matcher(
            dir.path(),
            false,
            false,
            Some(OsStr::new(".gitignore")),
            &["build/".to_string()],
            &[],
        );

        assert!(matcher.is_ignored(Path::new("build"), ft(&dir.path().join("build"))));
        assert!(matcher.is_ignored(
            Path::new("build/a.o"),
            ft(&dir.path().join("build").join("a.o"))
        ));
    }

    #[test]
    fn exclude_with_no_ignore_needs_no_ignore_file() {
        let dir = tempdir().unwrap();

        fs::write(dir.path().join(".gitignore"), "secret.txt\n").unwrap();
        fs::write(dir.path().join("secret.txt"), "secret").unwrap();
        fs::write(dir.path().join("keep.txt"), "keep").unwrap();

        let matcher = matcher(
            dir.path(),
            false,
            false,
            None,
            &["secret.txt".to_string()],
            &[],
        );

        assert!(matcher.is_ignored(Path::new("secret.txt"), ft(&dir.path().join("secret.txt"))));
        assert!(!matcher.is_ignored(Path::new("keep.txt"), ft(&dir.path().join("keep.txt"))));
    }

    #[test]
    fn include_overrides_gitignore() {
        let dir = tempdir().unwrap();

        fs::write(dir.path().join(".gitignore"), "*.log\n").unwrap();
        fs::write(dir.path().join("debug.log"), "log").unwrap();
        fs::write(dir.path().join("app.log"), "log").unwrap();

        let matcher = matcher(
            dir.path(),
            false,
            false,
            Some(OsStr::new(".gitignore")),
            &[],
            &["debug.log".to_string()],
        );

        assert!(!matcher.is_ignored(Path::new("debug.log"), ft(&dir.path().join("debug.log"))));
        assert!(matcher.is_ignored(Path::new("app.log"), ft(&dir.path().join("app.log"))));
    }

    #[test]
    fn include_reopens_ignored_directory() {
        let dir = tempdir().unwrap();

        fs::write(dir.path().join(".gitignore"), "target/\n").unwrap();
        fs::create_dir(dir.path().join("target")).unwrap();
        fs::write(dir.path().join("target").join("keep.txt"), "keep").unwrap();

        let matcher = matcher(
            dir.path(),
            false,
            false,
            Some(OsStr::new(".gitignore")),
            &[],
            &["target/".to_string()],
        );

        assert!(!matcher.is_ignored(Path::new("target"), ft(&dir.path().join("target"))));
        assert!(!matcher.is_ignored(
            Path::new("target/keep.txt"),
            ft(&dir.path().join("target").join("keep.txt"))
        ));
    }

    #[test]
    fn exclude_wins_over_include() {
        let dir = tempdir().unwrap();

        fs::write(dir.path().join("secret.txt"), "secret").unwrap();

        let matcher = matcher(
            dir.path(),
            false,
            false,
            None,
            &["secret.txt".to_string()],
            &["secret.txt".to_string()],
        );

        assert!(matcher.is_ignored(Path::new("secret.txt"), ft(&dir.path().join("secret.txt"))));
    }

    #[test]
    fn invalid_exclude_pattern_is_an_error() {
        let dir = tempdir().unwrap();

        let result = IgnoreMatcher::new(
            dir.path(),
            &IgnoreConfig {
                hide_git: false,
                hide_gitignore: false,
                ignore_file_name: None,
                require_ignore_file: false,
                exclude: &["{unclosed".to_string()],
                include: &[],
            },
        );

        assert!(result.is_err());
    }
}
