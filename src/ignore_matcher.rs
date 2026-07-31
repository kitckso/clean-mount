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
    hide_git: bool,
    hide_gitignore: bool,
}

impl IgnoreMatcher {
    pub fn new(
        root: &Path,
        hide_git: bool,
        hide_gitignore: bool,
        ignore_file_name: &OsStr,
    ) -> Self {
        let mut rules: Vec<ScopedRule> = Vec::new();

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

            if entry.file_name() == ignore_file_name
                && entry.file_type().is_some_and(|ft| ft.is_file())
            {
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

        Self {
            rules,
            hide_git,
            hide_gitignore,
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn ft(path: &Path) -> Option<FileType> {
        fs::symlink_metadata(path).ok().map(|m| m.file_type())
    }

    #[test]
    fn respects_root_gitignore() {
        let dir = tempdir().unwrap();

        fs::write(dir.path().join(".gitignore"), "secret.txt\ntarget/\n").unwrap();
        fs::write(dir.path().join("secret.txt"), "secret").unwrap();
        fs::write(dir.path().join("keep.txt"), "keep").unwrap();

        fs::create_dir(dir.path().join("target")).unwrap();
        fs::write(dir.path().join("target").join("a.txt"), "artifact").unwrap();

        let matcher = IgnoreMatcher::new(dir.path(), false, false, OsStr::new(".gitignore"));

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

        let matcher = IgnoreMatcher::new(dir.path(), false, false, OsStr::new(".gitignore"));

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

        let matcher = IgnoreMatcher::new(dir.path(), true, true, OsStr::new(".gitignore"));

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

        let matcher = IgnoreMatcher::new(dir.path(), false, false, OsStr::new(".gitignore"));

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

        let matcher = IgnoreMatcher::new(dir.path(), false, false, OsStr::new(".gitignore"));

        // .venv/ itself is ignored by root .gitignore
        assert!(matcher.is_ignored(Path::new(".venv"), ft(&dir.path().join(".venv"))));

        // app.py should NOT be ignored (the * from .venv/.gitignore must not leak)
        assert!(
            !matcher.is_ignored(Path::new("app.py"), ft(&dir.path().join("app.py"))),
            "app.py should NOT be ignored by .venv/.gitignore's * pattern"
        );
    }
}
