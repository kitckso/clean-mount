use fuser::{FileAttr, FileType, INodeNo};
use std::fs::Metadata;
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub fn file_attr_from_metadata(ino: u64, md: &Metadata) -> FileAttr {
    let kind = file_type_from(md.file_type());

    let atime = time_from_unix(md.atime(), md.atime_nsec());
    let mtime = time_from_unix(md.mtime(), md.mtime_nsec());
    let ctime = time_from_unix(md.ctime(), md.ctime_nsec());

    FileAttr {
        ino: INodeNo(ino),
        size: md.size(),
        blocks: md.blocks(),
        atime,
        mtime,
        ctime,
        crtime: mtime,
        kind,
        perm: (md.mode() & 0o7777) as u16,
        nlink: md.nlink() as u32,
        uid: md.uid(),
        gid: md.gid(),
        rdev: md.rdev() as u32,
        blksize: md.blksize() as u32,
        flags: 0,
    }
}

pub fn file_type_from(ft: std::fs::FileType) -> FileType {
    if ft.is_dir() {
        FileType::Directory
    } else if ft.is_symlink() {
        FileType::Symlink
    } else if ft.is_file() {
        FileType::RegularFile
    } else if ft.is_block_device() {
        FileType::BlockDevice
    } else if ft.is_char_device() {
        FileType::CharDevice
    } else if ft.is_fifo() {
        FileType::NamedPipe
    } else if ft.is_socket() {
        FileType::Socket
    } else {
        FileType::RegularFile
    }
}

fn time_from_unix(secs: i64, nsecs: i64) -> SystemTime {
    let secs = secs.max(0) as u64;
    let nsecs = nsecs.clamp(0, 1_000_000_000) as u32;
    UNIX_EPOCH + Duration::new(secs, nsecs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;
    use std::fs;
    use tempfile::tempdir;

    fn ft_for(path: &std::path::Path) -> std::fs::FileType {
        fs::symlink_metadata(path).unwrap().file_type()
    }

    #[test]
    fn file_type_from_maps_regular_file() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("f.txt");
        fs::write(&p, "x").unwrap();
        assert_eq!(file_type_from(ft_for(&p)), FileType::RegularFile);
    }

    #[test]
    fn file_type_from_maps_directory() {
        let dir = tempdir().unwrap();
        let sub = dir.path().join("d");
        fs::create_dir(&sub).unwrap();
        assert_eq!(file_type_from(ft_for(&sub)), FileType::Directory);
    }

    #[test]
    fn file_type_from_maps_symlink() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("target.txt");
        let link = dir.path().join("link");
        fs::write(&target, "x").unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert_eq!(file_type_from(ft_for(&link)), FileType::Symlink);
    }

    #[test]
    fn file_type_from_maps_named_pipe() {
        use std::os::unix::ffi::OsStrExt;
        let dir = tempdir().unwrap();
        let fifo = dir.path().join("fifo");
        unsafe {
            libc::mkfifo(
                CString::new(fifo.as_os_str().as_bytes()).unwrap().as_ptr(),
                0o644,
            );
        }
        assert_eq!(file_type_from(ft_for(&fifo)), FileType::NamedPipe);
        let _ = fs::remove_file(&fifo);
    }

    #[test]
    fn file_type_from_maps_socket() {
        let dir = tempdir().unwrap();
        let sock = dir.path().join("sock");
        let _ = std::os::unix::net::UnixListener::bind(&sock).unwrap();
        assert_eq!(file_type_from(ft_for(&sock)), FileType::Socket);
        let _ = fs::remove_file(&sock);
    }

    #[test]
    fn file_attr_from_metadata_populates_size_ino_and_kind() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("hello.txt");
        fs::write(&p, "hi").unwrap();
        let md = fs::symlink_metadata(&p).unwrap();

        let attr = file_attr_from_metadata(42, &md);

        assert_eq!(attr.ino, INodeNo(42));
        assert_eq!(attr.size, 2);
        assert_eq!(attr.kind, FileType::RegularFile);
        assert_eq!(attr.crtime, attr.mtime);
    }

    #[test]
    fn time_from_unix_returns_epoch_for_zero() {
        assert_eq!(time_from_unix(0, 0), UNIX_EPOCH);
    }

    #[test]
    fn time_from_unix_clamps_negative_seconds_to_zero() {
        assert_eq!(time_from_unix(-1, 0), UNIX_EPOCH);
        assert_eq!(time_from_unix(-1_000_000, 0), UNIX_EPOCH);
    }

    #[test]
    fn time_from_unix_clamps_nanos_above_one_billion() {
        let expected = UNIX_EPOCH + Duration::new(1, 1_000_000_000);
        assert_eq!(time_from_unix(1, 2_000_000_000), expected);
    }

    #[test]
    fn time_from_unix_clamps_negative_nanos_to_zero() {
        assert_eq!(time_from_unix(5, -42), UNIX_EPOCH + Duration::new(5, 0));
    }
}
