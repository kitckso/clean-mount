use crate::ignore_matcher::IgnoreMatcher;
use crate::inode::InodeTable;
use crate::metadata::{file_attr_from_metadata, file_type_from};
use fuser::{
    AccessFlags, Errno, FileHandle, FileType, Filesystem, FopenFlags, Generation, INodeNo,
    LockOwner, OpenAccMode, OpenFlags, ReplyAttr, ReplyData, ReplyDirectory, ReplyEmpty,
    ReplyEntry, ReplyOpen, ReplyStatfs, ReplyXattr, Request,
};
use std::collections::HashMap;
use std::ffi::{CString, OsStr};
use std::fs::{self, File};
use std::io;
use std::mem::MaybeUninit;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::FileExt;
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

const ROOT_INO: u64 = 1;

struct OpenHandle {
    file: File,
}

struct FsState {
    inodes: InodeTable,
    handles: HashMap<u64, OpenHandle>,
    next_fh: u64,
}

pub struct GitignoreMirrorFs {
    source: PathBuf,
    ignores: IgnoreMatcher,
    state: Mutex<FsState>,
    ttl: Duration,
}

impl GitignoreMirrorFs {
    pub fn new(
        source: PathBuf,
        ttl: Duration,
        hide_git: bool,
        hide_gitignore: bool,
        ignore_file: &str,
    ) -> anyhow::Result<Self> {
        let source = source.canonicalize()?;
        let ignores =
            IgnoreMatcher::new(&source, hide_git, hide_gitignore, OsStr::new(ignore_file))?;

        Ok(Self {
            source,
            ignores,
            state: Mutex::new(FsState {
                inodes: InodeTable::new(),
                handles: HashMap::new(),
                next_fh: 1,
            }),
            ttl,
        })
    }

    fn abs(&self, rel: &Path) -> PathBuf {
        if rel.as_os_str().is_empty() {
            self.source.clone()
        } else {
            self.source.join(rel)
        }
    }

    fn is_hidden(&self, rel: &Path, ft: Option<fs::FileType>) -> bool {
        self.ignores.is_ignored(rel, ft)
    }

    fn rel_or_enoent(&self, ino: INodeNo) -> Result<PathBuf, Errno> {
        self.state
            .lock()
            .unwrap()
            .inodes
            .path(ino.0)
            .map(|p| p.to_path_buf())
            .ok_or(Errno::ENOENT)
    }

    fn reply_entry_for_rel(&self, rel: &Path, reply: ReplyEntry, increment_lookup: bool) {
        let rel = normalize_rel(rel);
        let abs = self.abs(&rel);

        let md = match fs::symlink_metadata(&abs) {
            Ok(md) => md,
            Err(e) => {
                reply.error(err(&e));
                return;
            }
        };

        if self.is_hidden(&rel, Some(md.file_type())) {
            reply.error(Errno::ENOENT);
            return;
        }

        let ino = {
            let mut state = self.state.lock().unwrap();
            let ino = state.inodes.get_or_create(&rel);
            if increment_lookup {
                state.inodes.add_lookup(ino);
            }
            ino
        };

        let attr = file_attr_from_metadata(ino, &md);
        reply.entry(&self.ttl, &attr, Generation(0));
    }
}

fn err(e: &io::Error) -> Errno {
    Errno::from_i32(e.raw_os_error().unwrap_or(libc::EIO))
}

fn last_err() -> Errno {
    Errno::from_i32(
        io::Error::last_os_error()
            .raw_os_error()
            .unwrap_or(libc::EIO),
    )
}

fn normalize_rel(rel: &Path) -> PathBuf {
    let mut out = PathBuf::new();

    for comp in rel.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(c) => out.push(c),
            _ => {}
        }
    }

    out
}

fn normalize_lexically(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();

    for comp in path.components() {
        match comp {
            Component::Prefix(p) => out.push(p.as_os_str()),
            Component::RootDir => out.push(comp.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(c) => out.push(c),
        }
    }

    out
}

fn has_forbidden_name(name: &OsStr) -> bool {
    let b = name.as_bytes();
    b.is_empty() || b.contains(&0u8) || b.contains(&b'/')
}

fn is_dot(name: &OsStr) -> bool {
    name.as_bytes() == &b"."[..]
}

fn is_dotdot(name: &OsStr) -> bool {
    name.as_bytes() == &b".."[..]
}

fn validate_lookup_name(name: &OsStr) -> Result<(), Errno> {
    if has_forbidden_name(name) {
        Err(Errno::EINVAL)
    } else {
        Ok(())
    }
}

fn child_path(parent_rel: &Path, name: &OsStr) -> PathBuf {
    if parent_rel.as_os_str().is_empty() {
        PathBuf::from(name)
    } else {
        parent_rel.join(name)
    }
}

fn validate_open_flags(flags: OpenFlags) -> Result<(), Errno> {
    if flags.acc_mode() != OpenAccMode::O_RDONLY {
        return Err(Errno::EROFS);
    }
    if flags.0 & (libc::O_APPEND | libc::O_CREAT | libc::O_TRUNC) != 0 {
        return Err(Errno::EROFS);
    }
    Ok(())
}

fn is_path_escape(canonical: &Path, source: &Path) -> bool {
    !canonical.starts_with(source)
}

fn symlink_target_resolved(target: &Path, parent: &Path) -> PathBuf {
    let resolved = if target.is_absolute() {
        target.to_path_buf()
    } else {
        parent.join(target)
    };
    normalize_lexically(&resolved)
}

impl Filesystem for GitignoreMirrorFs {
    fn lookup(&self, _req: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEntry) {
        let parent_rel = match self.rel_or_enoent(parent) {
            Ok(p) => p,
            Err(e) => {
                reply.error(e);
                return;
            }
        };

        if is_dotdot(name) {
            let rel = normalize_rel(parent_rel.parent().unwrap_or(Path::new("")));
            self.reply_entry_for_rel(&rel, reply, true);
            return;
        }

        if is_dot(name) {
            self.reply_entry_for_rel(&parent_rel, reply, true);
            return;
        }

        if let Err(e) = validate_lookup_name(name) {
            reply.error(e);
            return;
        }

        let child_rel = child_path(&parent_rel, name);

        self.reply_entry_for_rel(&child_rel, reply, true);
    }

    fn forget(&self, _req: &Request, ino: INodeNo, nlookup: u64) {
        self.state.lock().unwrap().inodes.forget(ino.0, nlookup);
    }

    fn getattr(&self, _req: &Request, ino: INodeNo, _fh: Option<FileHandle>, reply: ReplyAttr) {
        let rel = match self.rel_or_enoent(ino) {
            Ok(p) => p,
            Err(e) => {
                reply.error(e);
                return;
            }
        };

        let abs = self.abs(&rel);

        let md = match fs::symlink_metadata(&abs) {
            Ok(md) => md,
            Err(e) => {
                reply.error(err(&e));
                return;
            }
        };

        if self.is_hidden(&rel, Some(md.file_type())) {
            reply.error(Errno::ENOENT);
            return;
        }

        let attr = file_attr_from_metadata(ino.0, &md);
        reply.attr(&self.ttl, &attr);
    }

    fn readlink(&self, _req: &Request, ino: INodeNo, reply: ReplyData) {
        let rel = match self.rel_or_enoent(ino) {
            Ok(p) => p,
            Err(e) => {
                reply.error(e);
                return;
            }
        };

        let abs = self.abs(&rel);

        let md = match fs::symlink_metadata(&abs) {
            Ok(md) => md,
            Err(e) => {
                reply.error(err(&e));
                return;
            }
        };

        if self.is_hidden(&rel, Some(md.file_type())) {
            reply.error(Errno::ENOENT);
            return;
        }

        if !md.file_type().is_symlink() {
            reply.error(Errno::EINVAL);
            return;
        }

        let target = match fs::read_link(&abs) {
            Ok(t) => t,
            Err(e) => {
                reply.error(err(&e));
                return;
            }
        };

        let parent = abs.parent().unwrap_or(Path::new(""));
        let normalized = symlink_target_resolved(&target, parent);

        if is_path_escape(&normalized, &self.source) {
            reply.error(Errno::EACCES);
            return;
        }

        reply.data(target.as_os_str().as_bytes());
    }

    fn opendir(&self, _req: &Request, ino: INodeNo, _flags: OpenFlags, reply: ReplyOpen) {
        let rel = match self.rel_or_enoent(ino) {
            Ok(p) => p,
            Err(e) => {
                reply.error(e);
                return;
            }
        };

        let abs = self.abs(&rel);

        let md = match fs::symlink_metadata(&abs) {
            Ok(md) => md,
            Err(e) => {
                reply.error(err(&e));
                return;
            }
        };

        if self.is_hidden(&rel, Some(md.file_type())) {
            reply.error(Errno::ENOENT);
            return;
        }

        if !md.is_dir() {
            reply.error(Errno::ENOTDIR);
            return;
        }

        reply.opened(FileHandle(0), FopenFlags::empty());
    }

    fn readdir(
        &self,
        _req: &Request,
        ino: INodeNo,
        _fh: FileHandle,
        offset: u64,
        mut reply: ReplyDirectory,
    ) {
        let rel = match self.rel_or_enoent(ino) {
            Ok(p) => p,
            Err(e) => {
                reply.error(e);
                return;
            }
        };

        let abs = self.abs(&rel);

        let md = match fs::symlink_metadata(&abs) {
            Ok(md) => md,
            Err(e) => {
                reply.error(err(&e));
                return;
            }
        };

        if self.is_hidden(&rel, Some(md.file_type())) {
            reply.error(Errno::ENOENT);
            return;
        }

        if !md.is_dir() {
            reply.error(Errno::ENOTDIR);
            return;
        }

        let mut entries: Vec<(u64, FileType, PathBuf)> = Vec::new();

        entries.push((ino.0, FileType::Directory, PathBuf::from(".")));

        let parent_ino = if rel.as_os_str().is_empty() {
            ROOT_INO
        } else {
            let parent_rel = normalize_rel(rel.parent().unwrap_or(Path::new("")));
            self.state.lock().unwrap().inodes.get_or_create(&parent_rel)
        };

        entries.push((parent_ino, FileType::Directory, PathBuf::from("..")));

        let read_dir = match fs::read_dir(&abs) {
            Ok(rd) => rd,
            Err(e) => {
                reply.error(err(&e));
                return;
            }
        };

        let mut children: Vec<(PathBuf, u64, FileType)> = Vec::new();

        for entry in read_dir {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    tracing::warn!(error = %e, "skipping unreadable directory entry");
                    continue;
                }
            };

            let name = entry.file_name();

            let child_rel = if rel.as_os_str().is_empty() {
                PathBuf::from(name.clone())
            } else {
                rel.join(&name)
            };

            let ft = entry.file_type().ok();

            if self.is_hidden(&child_rel, ft) {
                continue;
            }

            let child_ino = self.state.lock().unwrap().inodes.get_or_create(&child_rel);
            let kind = ft.map(file_type_from).unwrap_or(FileType::RegularFile);

            children.push((PathBuf::from(name), child_ino, kind));
        }

        children.sort_by(|a, b| a.0.cmp(&b.0));

        for (name, child_ino, kind) in children {
            entries.push((child_ino, kind, name));
        }

        for (idx, (entry_ino, kind, name)) in entries.into_iter().enumerate() {
            let next_offset = (idx + 1) as u64;

            if next_offset <= offset {
                continue;
            }

            if reply.add(INodeNo(entry_ino), next_offset, kind, name.as_os_str()) {
                break;
            }
        }

        reply.ok();
    }

    fn releasedir(
        &self,
        _req: &Request,
        _ino: INodeNo,
        _fh: FileHandle,
        _flags: OpenFlags,
        reply: ReplyEmpty,
    ) {
        reply.ok();
    }

    fn open(&self, _req: &Request, ino: INodeNo, flags: OpenFlags, reply: ReplyOpen) {
        if let Err(e) = validate_open_flags(flags) {
            reply.error(e);
            return;
        }

        let rel = match self.rel_or_enoent(ino) {
            Ok(p) => p,
            Err(e) => {
                reply.error(e);
                return;
            }
        };

        let abs = self.abs(&rel);

        let md = match fs::symlink_metadata(&abs) {
            Ok(md) => md,
            Err(e) => {
                reply.error(err(&e));
                return;
            }
        };

        if self.is_hidden(&rel, Some(md.file_type())) {
            reply.error(Errno::ENOENT);
            return;
        }

        if md.is_dir() {
            reply.error(Errno::EISDIR);
            return;
        }

        let canonical = match fs::canonicalize(&abs) {
            Ok(p) => p,
            Err(e) => {
                reply.error(err(&e));
                return;
            }
        };

        if is_path_escape(&canonical, &self.source) {
            reply.error(Errno::EACCES);
            return;
        }

        let file = match File::open(&canonical) {
            Ok(f) => f,
            Err(e) => {
                reply.error(err(&e));
                return;
            }
        };

        let fh = {
            let mut state = self.state.lock().unwrap();
            let fh = state.next_fh;
            state.next_fh = state.next_fh.saturating_add(1);
            state.handles.insert(fh, OpenHandle { file });
            fh
        };

        reply.opened(FileHandle(fh), FopenFlags::empty());
    }

    fn read(
        &self,
        _req: &Request,
        _ino: INodeNo,
        fh: FileHandle,
        offset: u64,
        size: u32,
        _flags: OpenFlags,
        _lock_owner: Option<LockOwner>,
        reply: ReplyData,
    ) {
        let state = self.state.lock().unwrap();
        let handle = match state.handles.get(&fh.0) {
            Some(h) => h,
            None => {
                reply.error(Errno::EBADF);
                return;
            }
        };

        let mut buf = vec![0u8; size as usize];

        match handle.file.read_at(&mut buf, offset) {
            Ok(n) => {
                buf.truncate(n);
                reply.data(&buf);
            }
            Err(e) => reply.error(err(&e)),
        }
    }

    fn release(
        &self,
        _req: &Request,
        _ino: INodeNo,
        fh: FileHandle,
        _flags: OpenFlags,
        _lock_owner: Option<LockOwner>,
        _flush: bool,
        reply: ReplyEmpty,
    ) {
        self.state.lock().unwrap().handles.remove(&fh.0);
        reply.ok();
    }

    fn flush(
        &self,
        _req: &Request,
        _ino: INodeNo,
        _fh: FileHandle,
        _lock_owner: LockOwner,
        reply: ReplyEmpty,
    ) {
        reply.ok();
    }

    fn fsync(
        &self,
        _req: &Request,
        _ino: INodeNo,
        _fh: FileHandle,
        _datasync: bool,
        reply: ReplyEmpty,
    ) {
        reply.ok();
    }

    fn access(&self, _req: &Request, ino: INodeNo, mask: AccessFlags, reply: ReplyEmpty) {
        let rel = match self.rel_or_enoent(ino) {
            Ok(p) => p,
            Err(e) => {
                reply.error(e);
                return;
            }
        };

        if mask.contains(AccessFlags::W_OK) {
            reply.error(Errno::EROFS);
            return;
        }

        let abs = self.abs(&rel);

        let md = match fs::symlink_metadata(&abs) {
            Ok(md) => md,
            Err(e) => {
                reply.error(err(&e));
                return;
            }
        };

        if self.is_hidden(&rel, Some(md.file_type())) {
            reply.error(Errno::ENOENT);
            return;
        }

        let canonical = match fs::canonicalize(&abs) {
            Ok(p) => p,
            Err(e) => {
                reply.error(err(&e));
                return;
            }
        };

        if is_path_escape(&canonical, &self.source) {
            reply.error(Errno::EACCES);
            return;
        }

        let c_path = match CString::new(canonical.as_os_str().as_bytes()) {
            Ok(p) => p,
            Err(_) => {
                reply.error(Errno::EINVAL);
                return;
            }
        };

        let rc = unsafe { libc::access(c_path.as_ptr(), mask.bits()) };

        if rc == 0 {
            reply.ok();
        } else {
            reply.error(last_err());
        }
    }

    fn statfs(&self, _req: &Request, _ino: INodeNo, reply: ReplyStatfs) {
        let c_path = match CString::new(self.source.as_os_str().as_bytes()) {
            Ok(p) => p,
            Err(_) => {
                reply.error(Errno::EINVAL);
                return;
            }
        };

        let mut st = MaybeUninit::<libc::statvfs>::zeroed();

        let rc = unsafe { libc::statvfs(c_path.as_ptr(), st.as_mut_ptr()) };

        if rc != 0 {
            reply.error(last_err());
            return;
        }

        let st = unsafe { st.assume_init() };

        reply.statfs(
            st.f_blocks,
            st.f_bfree,
            st.f_bavail,
            st.f_files,
            st.f_ffree,
            st.f_bsize as u32,
            st.f_namemax as u32,
            st.f_frsize as u32,
        );
    }

    fn getxattr(
        &self,
        _req: &Request,
        _ino: INodeNo,
        _name: &OsStr,
        _size: u32,
        reply: ReplyXattr,
    ) {
        reply.error(Errno::NO_XATTR);
    }

    fn listxattr(&self, _req: &Request, _ino: INodeNo, size: u32, reply: ReplyXattr) {
        if size == 0 {
            reply.size(0);
        } else {
            reply.data(&[]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::symlink;
    use std::path::PathBuf;
    use tempfile::tempdir;

    #[test]
    fn new_returns_error_for_nonexistent_source() {
        let dir = tempdir().unwrap();
        let missing = dir.path().join("does_not_exist");

        let result =
            GitignoreMirrorFs::new(missing, Duration::from_secs(1), false, false, ".gitignore");

        assert!(result.is_err(), "expected Err for non-existent source");
    }

    #[test]
    fn new_returns_error_for_nonexistent_source_unwraps_io_error() {
        let dir = tempdir().unwrap();
        let missing = dir.path().join("does_not_exist");

        let result =
            GitignoreMirrorFs::new(missing, Duration::from_secs(1), false, false, ".gitignore");

        assert!(result.is_err(), "expected Err for non-existent source");
    }

    #[test]
    fn new_succeeds_for_empty_directory() {
        let dir = tempdir().unwrap();

        let result = GitignoreMirrorFs::new(
            dir.path().to_path_buf(),
            Duration::from_secs(1),
            false,
            false,
            ".gitignore",
        );

        assert!(result.is_ok());
    }

    fn assert_erofs(r: Result<(), Errno>) {
        let err: i32 = r.unwrap_err().into();
        assert_eq!(err, libc::EROFS);
    }

    #[test]
    fn validate_open_flags_accepts_odonly() {
        assert!(validate_open_flags(OpenFlags(libc::O_RDONLY)).is_ok());
        assert!(validate_open_flags(OpenFlags(libc::O_RDONLY | libc::O_NONBLOCK)).is_ok());
    }

    #[test]
    fn validate_open_flags_rejects_owronly() {
        assert_erofs(validate_open_flags(OpenFlags(libc::O_WRONLY)));
    }

    #[test]
    fn validate_open_flags_rejects_ordwr() {
        assert_erofs(validate_open_flags(OpenFlags(libc::O_RDWR)));
    }

    #[test]
    fn validate_open_flags_rejects_owronly_combined_with_readonly() {
        assert_erofs(validate_open_flags(OpenFlags(
            libc::O_WRONLY | libc::O_RDONLY,
        )));
    }

    #[test]
    fn validate_open_flags_rejects_append() {
        assert_erofs(validate_open_flags(OpenFlags(
            libc::O_RDONLY | libc::O_APPEND,
        )));
    }

    #[test]
    fn validate_open_flags_rejects_creat() {
        assert_erofs(validate_open_flags(OpenFlags(
            libc::O_RDONLY | libc::O_CREAT,
        )));
    }

    #[test]
    fn validate_open_flags_rejects_trunc() {
        assert_erofs(validate_open_flags(OpenFlags(
            libc::O_RDONLY | libc::O_TRUNC,
        )));
    }

    #[test]
    fn is_path_escape_returns_false_for_source_itself() {
        let dir = tempdir().unwrap();
        let canonical = fs::canonicalize(dir.path()).unwrap();
        assert!(!is_path_escape(&canonical, &canonical));
    }

    #[test]
    fn is_path_escape_returns_false_for_child_of_source() {
        let dir = tempdir().unwrap();
        let child = dir.path().join("child");
        fs::create_dir(&child).unwrap();
        let canonical = fs::canonicalize(&child).unwrap();
        let parent = canonical.parent().unwrap();
        assert!(!is_path_escape(&canonical, parent));
    }

    #[test]
    fn is_path_escape_returns_true_for_sibling() {
        let parent = tempdir().unwrap();
        let a = parent.path().join("a");
        let b = parent.path().join("b");
        fs::create_dir(&a).unwrap();
        fs::create_dir(&b).unwrap();
        let canonical_a = fs::canonicalize(&a).unwrap();
        let canonical_b = fs::canonicalize(&b).unwrap();
        assert!(is_path_escape(&canonical_b, &canonical_a));
    }

    #[test]
    fn is_path_escape_returns_true_for_parent() {
        let parent = tempdir().unwrap();
        let child = parent.path().join("child");
        fs::create_dir(&child).unwrap();
        let canonical_child = fs::canonicalize(&child).unwrap();
        let canonical_parent = fs::canonicalize(parent.path()).unwrap();
        assert!(is_path_escape(&canonical_parent, &canonical_child));
    }

    #[test]
    fn symlink_target_resolved_handles_absolute_target() {
        let target = Path::new("/etc/passwd");
        let resolved = symlink_target_resolved(target, Path::new("/anywhere"));
        assert_eq!(resolved, PathBuf::from("/etc/passwd"));
    }

    #[test]
    fn symlink_target_resolved_joins_relative_target_to_parent() {
        let target = Path::new("sibling");
        let parent = Path::new("/a/b");
        let resolved = symlink_target_resolved(target, parent);
        assert_eq!(resolved, PathBuf::from("/a/b/sibling"));
    }

    #[test]
    fn symlink_target_resolved_normalizes_dotdot() {
        let target = Path::new("../outside");
        let parent = Path::new("/a/b");
        let resolved = symlink_target_resolved(target, parent);
        assert_eq!(resolved, PathBuf::from("/a/outside"));
    }

    #[test]
    fn symlink_escape_simulation_blocked_by_helper() {
        let outside_parent = tempdir().unwrap();
        let secret_dir = outside_parent.path().join("outside");
        fs::create_dir(&secret_dir).unwrap();
        let secret = secret_dir.join("secret.txt");
        fs::write(&secret, "secret").unwrap();

        let source_dir = tempdir().unwrap();
        let link = source_dir.path().join("escape");
        let abs_target = fs::canonicalize(&secret).unwrap();
        symlink(&abs_target, &link).unwrap();

        let source_root = fs::canonicalize(source_dir.path()).unwrap();
        let canonical = fs::canonicalize(&link).unwrap();

        assert!(
            is_path_escape(&canonical, &source_root),
            "symlink resolving to outside source_root should be detected as escape"
        );
    }

    #[test]
    fn symlink_inside_source_is_not_an_escape() {
        let source_dir = tempdir().unwrap();
        let target = source_dir.path().join("target.txt");
        fs::write(&target, "ok").unwrap();
        let link = source_dir.path().join("link");
        symlink(&target, &link).unwrap();

        let source_root = fs::canonicalize(source_dir.path()).unwrap();
        let canonical = fs::canonicalize(&link).unwrap();

        assert!(!is_path_escape(&canonical, &source_root));
    }

    #[test]
    fn is_dot_recognises_only_dot() {
        assert!(is_dot(OsStr::new(".")));
        assert!(!is_dot(OsStr::new("..")));
        assert!(!is_dot(OsStr::new("a")));
        assert!(!is_dot(OsStr::new(".hidden")));
    }

    #[test]
    fn is_dotdot_recognises_only_double_dot() {
        assert!(is_dotdot(OsStr::new("..")));
        assert!(!is_dotdot(OsStr::new(".")));
        assert!(!is_dotdot(OsStr::new("a")));
        assert!(!is_dotdot(OsStr::new("...x")));
    }

    #[test]
    fn has_forbidden_name_rejects_empty_nul_and_slash() {
        assert!(has_forbidden_name(OsStr::new("")));
        assert!(has_forbidden_name(OsStr::from_bytes(b"foo\0bar")));
        assert!(has_forbidden_name(OsStr::from_bytes(b"foo/bar")));
    }

    #[test]
    fn has_forbidden_name_accepts_normal_names() {
        assert!(!has_forbidden_name(OsStr::new("foo")));
        assert!(!has_forbidden_name(OsStr::new(".hidden")));
        assert!(!has_forbidden_name(OsStr::new("a.b.c")));
    }

    fn assert_einval(r: Result<(), Errno>) {
        let err: i32 = r.unwrap_err().into();
        assert_eq!(err, libc::EINVAL);
    }

    #[test]
    fn validate_lookup_name_errors_on_empty() {
        assert_einval(validate_lookup_name(OsStr::new("")));
    }

    #[test]
    fn validate_lookup_name_errors_on_nul_byte() {
        assert_einval(validate_lookup_name(OsStr::from_bytes(b"a\0b")));
    }

    #[test]
    fn validate_lookup_name_errors_on_slash() {
        assert_einval(validate_lookup_name(OsStr::from_bytes(b"a/b")));
    }

    #[test]
    fn validate_lookup_name_accepts_dot_and_dotdot() {
        assert!(validate_lookup_name(OsStr::new(".")).is_ok());
        assert!(validate_lookup_name(OsStr::new("..")).is_ok());
    }

    #[test]
    fn validate_lookup_name_accepts_normal_names() {
        assert!(validate_lookup_name(OsStr::new("foo.txt")).is_ok());
    }

    #[test]
    fn child_path_uses_name_when_parent_is_root() {
        let parent = Path::new("");
        let result = child_path(parent, OsStr::new("foo"));
        assert_eq!(result, PathBuf::from("foo"));
    }

    #[test]
    fn child_path_joins_when_parent_is_nested() {
        let parent = Path::new("a/b");
        let result = child_path(parent, OsStr::new("c"));
        assert_eq!(result, PathBuf::from("a/b/c"));
    }
}
