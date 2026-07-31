use anyhow::{bail, Context, Result};
use clap::CommandFactory;
use clap::Parser;
use fuser::{BackgroundSession, Config, MountOption, SessionACL};
use std::collections::HashSet;
use std::ffi::OsStr;
use std::fs::DirEntry;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tempfile::TempDir;
use tracing_subscriber::EnvFilter;

static SIGINT_RECEIVED: AtomicBool = AtomicBool::new(false);

extern "C" fn handle_sigint(_: i32) {
    SIGINT_RECEIVED.store(true, Ordering::Release);
}

mod cli;
mod fs;
mod ignore_matcher;
mod inode;
mod metadata;
mod registry;

use crate::cli::{Cli, Commands, CommonOpts};
use crate::fs::GitignoreMirrorFs;
use crate::ignore_matcher::IgnoreMatcher;
use crate::registry::MountRegistry;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    unsafe {
        let handler = handle_sigint as extern "C" fn(i32) as usize;
        libc::signal(libc::SIGINT, handler);
        libc::signal(libc::SIGTERM, handler);
    }

    let cli = Cli::parse();

    if cfg!(target_os = "linux") && !Path::new("/dev/fuse").exists() {
        bail!("/dev/fuse not found. Install fuse3 and ensure the fuse kernel module is available.");
    }

    match cli.command {
        None => {
            let source = cli.source.context("SOURCE is required")?;
            cmd_mount(&source, cli.mountpoint, &cli.opts, false)
        }
        Some(Commands::Mount {
            source,
            mountpoint,
            opts,
            daemon,
        }) => cmd_mount(&source, mountpoint, &opts, daemon),
        Some(Commands::List {
            source,
            opts,
            tree,
            summary,
        }) => cmd_list(&source, &opts, tree, summary),
        Some(Commands::Cp { source, dest, opts }) => cmd_cp(&source, &dest, &opts),
        Some(Commands::Exec {
            source,
            command,
            opts,
        }) => cmd_exec(&source, &command, &opts),
        Some(Commands::Open { source, opts }) => cmd_open(&source, &opts),
        Some(Commands::Tar {
            source,
            output,
            opts,
        }) => cmd_tar(&source, &output, &opts),
        Some(Commands::Zip {
            source,
            output,
            opts,
        }) => cmd_zip(&source, &output, &opts),
        Some(Commands::Status) => cmd_status(),
        Some(Commands::Stop { pid, mountpoint }) => cmd_stop(pid, mountpoint),
        Some(Commands::Complete { shell, install }) => {
            if install {
                cmd_complete_install(shell)?;
            } else {
                cmd_complete(shell)?;
            }
            Ok(())
        }
    }
}

fn validate_source(source: &Path) -> Result<PathBuf> {
    let source = source
        .canonicalize()
        .context("failed to canonicalize source directory")?;
    if !source.is_dir() {
        bail!("source must be a directory: {}", source.display());
    }
    Ok(source)
}

fn create_temp_mountpoint(source: &Path) -> Result<TempDir> {
    let prefix = source.file_name().unwrap_or(OsStr::new("clean-mount"));
    tempfile::Builder::new()
        .prefix(&format!("{}-", prefix.to_string_lossy()))
        .tempdir()
        .context("failed to create temporary mountpoint")
}

fn copy_to_clipboard(path: &Path, clipboard: bool) {
    if !clipboard {
        return;
    }
    let text = path.to_string_lossy();
    let result = std::process::Command::new("xclip")
        .args(["-selection", "clipboard"])
        .stdin(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(text.as_bytes())?;
            }
            child.wait()
        });
    match result {
        Ok(status) if status.success() => eprintln!("path copied to clipboard"),
        Ok(status) => eprintln!("warning: xclip exited with {status}"),
        Err(e) => eprintln!("warning: failed to copy to clipboard: {e}"),
    }
}

fn acl_from_opts(opts: &CommonOpts) -> SessionACL {
    if opts.allow_root {
        SessionACL::RootAndOwner
    } else if opts.allow_other {
        if !opts.default_permissions {
            tracing::warn!(
                "--allow-other without --default-permissions may expose files using the FUSE daemon owner's permissions"
            );
        }
        SessionACL::All
    } else {
        SessionACL::Owner
    }
}

fn build_fs(source: &Path, opts: &CommonOpts) -> Result<GitignoreMirrorFs> {
    GitignoreMirrorFs::new(
        source,
        Duration::from_secs(opts.ttl_secs),
        opts.hide_git,
        opts.hide_gitignore,
        &opts.ignore_file,
    )
    .context("failed to initialize filesystem")
}

fn build_config(source: &Path, opts: &CommonOpts) -> Config {
    let mut mount_options = vec![
        MountOption::RO,
        MountOption::FSName(format!("clean-mount:{}", source.display())),
    ];

    if opts.default_permissions {
        mount_options.push(MountOption::DefaultPermissions);
    }

    let mut config = Config::default();
    config.mount_options = mount_options;
    config.acl = acl_from_opts(opts);
    config
}

fn validate_mountpoint(mp: &Path, source: &Path) -> Result<PathBuf> {
    let mp = mp
        .canonicalize()
        .context("failed to canonicalize mountpoint directory")?;
    if !mp.is_dir() {
        bail!("mountpoint must be an existing directory: {}", mp.display());
    }
    let mut entries = mp
        .read_dir()
        .context("failed to read mountpoint directory")?;
    if entries.next().is_some() {
        bail!("mountpoint must be empty before mounting: {}", mp.display());
    }
    if mp.starts_with(source) || source.starts_with(&mp) {
        bail!("source and mountpoint must not be nested inside each other");
    }
    Ok(mp)
}

fn cmd_mount(
    source: &Path,
    mountpoint: Option<PathBuf>,
    opts: &CommonOpts,
    daemon: bool,
) -> Result<()> {
    let source = validate_source(source)?;

    if daemon {
        let mp = mountpoint.context("--daemon requires an explicit mountpoint")?;
        let mp = validate_mountpoint(&mp, &source)?;

        let mut fds = [0; 2];
        if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
            bail!("failed to create sync pipe");
        }
        let (read_fd, write_fd) = (fds[0], fds[1]);

        match unsafe { libc::fork() } {
            -1 => bail!("fork failed"),
            0 => {
                unsafe {
                    libc::close(read_fd);
                    libc::setsid();
                }
                redirect_stdio()?;
                let registry = MountRegistry::new()?;
                if let Err(e) = registry.register(&source, &mp, std::process::id()) {
                    tracing::warn!(error = %e, "failed to register daemon mount");
                }
                let session = mount_and_spawn(&source, opts, &mp);
                let status: u8 = u8::from(session.is_err());
                unsafe {
                    let _ = libc::write(
                        write_fd,
                        (&raw const status).cast::<libc::c_void>(),
                        std::mem::size_of::<u8>(),
                    );
                    libc::close(write_fd);
                }
                match session {
                    Ok(session) => {
                        wait_for_unmount(session);
                        if let Err(e) = registry.unregister(std::process::id()) {
                            tracing::warn!(error = %e, "failed to unregister daemon mount");
                        }
                        std::process::exit(0);
                    }
                    Err(e) => {
                        if let Err(e) = registry.unregister(std::process::id()) {
                            tracing::warn!(error = %e, "failed to unregister daemon mount");
                        }
                        tracing::error!("daemon mount failed: {e}");
                        std::process::exit(1);
                    }
                }
            }
            pid => {
                unsafe { libc::close(write_fd) };
                let mut status = [0u8; 1];
                let n = unsafe {
                    libc::read(
                        read_fd,
                        status.as_mut_ptr().cast::<libc::c_void>(),
                        status.len(),
                    )
                };
                unsafe { libc::close(read_fd) };
                if n == 1 && status[0] == 0 {
                    println!("{pid}");
                    std::process::exit(0);
                }
                bail!("daemon failed to mount");
            }
        }
    }

    if let Some(mp) = mountpoint {
        let mp = validate_mountpoint(&mp, &source)?;
        mount_blocking(&source, opts, &mp)
    } else {
        let tmp = create_temp_mountpoint(&source)?;
        println!("{}", tmp.path().display());
        copy_to_clipboard(tmp.path(), opts.clipboard);
        mount_blocking(&source, opts, tmp.path())
    }
}

fn mount_blocking(source: &Path, opts: &CommonOpts, mountpoint: &Path) -> Result<()> {
    let fs = build_fs(source, opts)?;
    let config = build_config(source, opts);

    tracing::info!(
        source = %source.display(),
        mountpoint = %mountpoint.display(),
        ttl_secs = opts.ttl_secs,
        hide_git = opts.hide_git,
        hide_gitignore = opts.hide_gitignore,
        "mounting read-only gitignore-filtered FUSE filesystem"
    );

    let session = fuser::spawn_mount(fs, mountpoint, &config).context("FUSE mount failed")?;
    wait_for_unmount(session);
    Ok(())
}

fn wait_for_unmount(session: BackgroundSession) {
    while !SIGINT_RECEIVED.load(Ordering::Acquire) && !session.guard.is_finished() {
        std::thread::sleep(Duration::from_millis(100));
    }
    drop(session);
    tracing::info!("filesystem unmounted");
}

fn cmd_cp(source: &Path, dest: &Path, opts: &CommonOpts) -> Result<()> {
    let source = validate_source(source)?;

    if dest.exists() {
        bail!("destination already exists: {}", dest.display());
    }

    let mountpoint = create_temp_mountpoint(&source)?;
    let session = mount_and_spawn(&source, opts, mountpoint.path())?;
    copy_to_clipboard(mountpoint.path(), opts.clipboard);

    tracing::info!(
        source = %source.display(),
        dest = %dest.display(),
        "copying filtered files"
    );

    let status = Command::new("cp")
        .args(["-a", "--"])
        .arg(mountpoint.path())
        .arg(dest)
        .status()
        .context("failed to execute cp")?;

    drop(session);

    if !status.success() {
        bail!("cp exited with status: {status}");
    }

    tracing::info!("copy complete");
    Ok(())
}

fn cmd_exec(source: &Path, command: &[String], opts: &CommonOpts) -> Result<()> {
    let Some((cmd, args)) = command.split_first() else {
        bail!("exec requires a command to run");
    };

    let source = validate_source(source)?;

    let mountpoint = create_temp_mountpoint(&source)?;
    let session = mount_and_spawn(&source, opts, mountpoint.path())?;
    copy_to_clipboard(mountpoint.path(), opts.clipboard);

    let mount_path_str = mountpoint.path().to_string_lossy().into_owned();

    let args: Vec<String> = args
        .iter()
        .map(|a| {
            a.replace("{MOUNT}", &mount_path_str)
                .replace("{CLEAN_MOUNT}", &mount_path_str)
        })
        .collect();

    tracing::info!(
        source = %source.display(),
        command = %cmd,
        mountpoint = %mount_path_str,
        "executing command against filtered view"
    );

    let status = Command::new(cmd)
        .args(&args)
        .current_dir(mountpoint.path())
        .status()
        .context("failed to execute command")?;

    drop(session);

    if !status.success() {
        bail!("command exited with status: {status}");
    }

    Ok(())
}

fn cmd_open(source: &Path, opts: &CommonOpts) -> Result<()> {
    let source = validate_source(source)?;
    let mountpoint = create_temp_mountpoint(&source)?;
    let mount_path = mountpoint.path().to_path_buf();

    let session = mount_and_spawn(&source, opts, &mount_path)?;
    copy_to_clipboard(&mount_path, opts.clipboard);

    println!("{}", mount_path.display());

    open_file_manager(&mount_path);

    wait_for_unmount(session);

    Ok(())
}

fn open_file_manager(path: &Path) {
    let _ = Command::new("xdg-open").arg(path).status();
}

fn cmd_tar(source: &Path, output: &Path, opts: &CommonOpts) -> Result<()> {
    let source = validate_source(source)?;
    let mountpoint = create_temp_mountpoint(&source)?;
    let session = mount_and_spawn(&source, opts, mountpoint.path())?;

    tracing::info!(
        source = %source.display(),
        output = %output.display(),
        "creating tarball"
    );

    let status = Command::new("tar")
        .args(["-acf", &output.to_string_lossy(), "-C"])
        .arg(mountpoint.path())
        .arg(".")
        .status()
        .context("failed to execute tar")?;

    drop(session);

    if !status.success() {
        bail!("tar exited with status: {status}");
    }

    tracing::info!("tarball created");
    Ok(())
}

fn cmd_zip(source: &Path, output: &Path, opts: &CommonOpts) -> Result<()> {
    let source = validate_source(source)?;
    let mountpoint = create_temp_mountpoint(&source)?;
    let session = mount_and_spawn(&source, opts, mountpoint.path())?;

    tracing::info!(
        source = %source.display(),
        output = %output.display(),
        "creating zip archive"
    );

    let status = Command::new("zip")
        .args(["-r", &output.to_string_lossy()])
        .arg(".")
        .current_dir(mountpoint.path())
        .status()
        .context("failed to execute zip")?;

    drop(session);

    if !status.success() {
        bail!("zip exited with status: {status}");
    }

    tracing::info!("zip archive created");
    Ok(())
}

fn cmd_complete(shell: Option<clap_complete::Shell>) -> Result<()> {
    use clap_complete::generate;
    use std::io::Write;
    let shell = shell
        .or_else(clap_complete::Shell::from_env)
        .unwrap_or(clap_complete::Shell::Bash);
    let mut cmd = Cli::command();
    let mut buf = Vec::new();
    generate(shell, &mut cmd, "clean-mount", &mut buf);
    let output = String::from_utf8(buf)?;
    let _ = writeln!(std::io::stdout(), "{output}");
    Ok(())
}

fn cmd_complete_install(shell: Option<clap_complete::Shell>) -> Result<()> {
    use std::io::Write;
    let explicit = shell.is_some();
    let shell = shell
        .or_else(clap_complete::Shell::from_env)
        .unwrap_or(clap_complete::Shell::Bash);
    let home = std::env::var("HOME").context("$HOME not set")?;
    let home = Path::new(&home);

    let rc_file: PathBuf = match shell {
        clap_complete::Shell::Bash => home.join(".bashrc"),
        clap_complete::Shell::Zsh => home.join(".zshrc"),
        clap_complete::Shell::Fish => home.join(".config/fish/config.fish"),
        clap_complete::Shell::Elvish => home.join(".config/elvish/rc.elv"),
        _ => bail!("auto-install not supported for {shell}. Install manually by adding `eval \"$(clean-mount complete {shell})\"` to your shell's rc file."),
    };

    let bin = std::env::args().next().map_or_else(
        || "clean-mount".to_string(),
        |p| {
            Path::new(&p)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        },
    );
    let line = if explicit {
        format!("eval \"$({bin} complete {shell})\"")
    } else {
        format!("eval \"$({bin} complete)\"")
    };
    let comment = "# clean-mount shell completion";

    if rc_file.exists() {
        let content = std::fs::read_to_string(&rc_file)?;
        if content.contains(&line) || content.contains(comment) {
            eprintln!("completions already installed in {}", rc_file.display());
            return Ok(());
        }
    }

    if let Some(parent) = rc_file.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&rc_file)?;

    writeln!(file)?;
    writeln!(file, "{comment}")?;
    writeln!(file, "{line}")?;

    eprintln!("completions installed in {}", rc_file.display());
    eprintln!(
        "run `source {}` or restart your shell to activate",
        rc_file.display()
    );
    Ok(())
}

fn redirect_stdio() -> Result<()> {
    use std::os::unix::io::IntoRawFd;
    let devnull = std::fs::File::open("/dev/null")?;
    let fd = devnull.into_raw_fd();
    unsafe {
        libc::dup2(fd, 0);
        libc::dup2(fd, 1);
        libc::dup2(fd, 2);
        libc::close(fd);
    }
    Ok(())
}

fn run_fusermount_u(mountpoint: &Path) -> Result<()> {
    let fusermount = Command::new("fusermount3")
        .args(["-u", &mountpoint.to_string_lossy()])
        .status();

    let result = match fusermount {
        Ok(status) if status.success() => return Ok(()),
        Ok(_) | Err(_) => Command::new("umount").arg(mountpoint).status(),
    };

    match result {
        Ok(status) if status.success() => Ok(()),
        Ok(_) => {
            if !is_mounted(mountpoint) {
                return Ok(());
            }
            bail!("unmount failed; the mount may be in use");
        }
        Err(e) => Err(e).context("failed to run fusermount3 or umount"),
    }
}

#[cfg(target_os = "linux")]
fn is_mounted(path: &Path) -> bool {
    let Ok(content) = std::fs::read_to_string("/proc/self/mountinfo") else {
        return true;
    };
    let path_str = path.to_string_lossy();
    content
        .lines()
        .any(|line| line.split_whitespace().nth(4) == Some(path_str.as_ref()))
}

#[cfg(not(target_os = "linux"))]
fn is_mounted(path: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;

    let mut mntbufp: *mut libc::statfs = std::ptr::null_mut();
    let count = unsafe { libc::getmntinfo(&mut mntbufp, libc::MNT_NOWAIT) };
    if count <= 0 {
        return true;
    }
    let path_bytes = path.as_os_str().as_bytes();
    unsafe {
        for i in 0..count {
            let name = std::ffi::CStr::from_ptr((*mntbufp.add(i as usize)).f_mntonname.as_ptr());
            if name.to_bytes() == path_bytes {
                return true;
            }
        }
    }
    false
}

fn cmd_status() -> Result<()> {
    let registry = MountRegistry::new()?;
    let entries = registry.list()?;
    if entries.is_empty() {
        println!("no active mounts");
        return Ok(());
    }
    println!(
        "{:>6}  {:<24}  {:<40}  UPTIME",
        "PID", "SOURCE", "MOUNTPOINT"
    );
    for e in &entries {
        println!(
            "{:>6}  {:<24}  {:<40}  {}",
            e.pid,
            e.source,
            e.mountpoint,
            e.uptime_str()
        );
    }
    Ok(())
}

fn cmd_stop(pid: Option<u32>, mountpoint: Option<PathBuf>) -> Result<()> {
    let registry = MountRegistry::new()?;

    if let Some(pid) = pid {
        let entry = registry
            .lookup_by_pid(pid)?
            .with_context(|| format!("no registered mount found for PID {pid}"))?;
        run_fusermount_u(Path::new(&entry.mountpoint))?;
        registry.unregister(pid)?;
    } else if let Some(mp) = mountpoint {
        let mp = mp
            .canonicalize()
            .context("failed to canonicalize mountpoint directory")?;
        run_fusermount_u(&mp)?;
        for entry in registry.lookup_by_mountpoint(&mp)? {
            registry.unregister(entry.pid)?;
        }
    } else {
        bail!("use --pid <PID> or provide a mountpoint");
    }

    Ok(())
}

fn mount_and_spawn(
    source: &Path,
    opts: &CommonOpts,
    mountpoint: &Path,
) -> Result<BackgroundSession> {
    let fs = build_fs(source, opts)?;
    let config = build_config(source, opts);

    fuser::spawn_mount(fs, mountpoint, &config).context("FUSE mount failed")
}

const PRINT_LIMIT: u64 = 2000;

struct ListCtx {
    shown: u64,
    ignored: u64,
    size: u64,
    seen: HashSet<(u64, u64)>,
}

impl ListCtx {
    fn new() -> Self {
        Self {
            shown: 0,
            ignored: 0,
            size: 0,
            seen: HashSet::new(),
        }
    }
}

struct Lister<'a> {
    matcher: &'a IgnoreMatcher,
    max_depth: Option<u64>,
    printed: u64,
    ctx: Option<ListCtx>,
}

fn rel_join(rel: &Path, name: &OsStr) -> PathBuf {
    if rel.as_os_str().is_empty() {
        PathBuf::from(name)
    } else {
        rel.join(name)
    }
}

fn cmd_list(source: &Path, opts: &CommonOpts, tree: bool, summary: bool) -> Result<()> {
    let source = validate_source(source)?;
    let matcher = IgnoreMatcher::new(
        &source,
        opts.hide_git,
        opts.hide_gitignore,
        OsStr::new(&opts.ignore_file),
    );

    let mut lister = Lister {
        matcher: &matcher,
        max_depth: if tree { None } else { Some(0) },
        printed: 0,
        ctx: summary.then(ListCtx::new),
    };
    let truncated = lister.run(&source)?;
    let Lister { ctx, printed, .. } = lister;

    if let Some(ctx) = ctx {
        if printed < ctx.shown {
            println!(
                "{} files ({} ignored, {} total, {} printed)",
                ctx.shown,
                ctx.ignored,
                format_size(ctx.size),
                printed,
            );
        } else {
            println!(
                "{} files ({} ignored, {} total)",
                ctx.shown,
                ctx.ignored,
                format_size(ctx.size),
            );
        }
    } else if truncated {
        println!("... (truncated at {PRINT_LIMIT} files)");
    }

    Ok(())
}

fn add_size(md: &std::fs::Metadata, seen: &mut HashSet<(u64, u64)>, total: &mut u64) {
    if seen.insert((md.dev(), md.ino())) {
        *total += md.blocks().max(1) * 512;
    }
}

impl Lister<'_> {
    fn run(&mut self, root: &Path) -> Result<bool> {
        self.list_tree(root, Path::new(""), "", 0)
    }

    fn list_tree(&mut self, abs_dir: &Path, rel: &Path, indent: &str, depth: u64) -> Result<bool> {
        let mut entries: Vec<_> = std::fs::read_dir(abs_dir)?.filter_map(Result::ok).collect();
        entries.sort_by_key(DirEntry::file_name);

        for entry in &entries {
            if self.printed >= PRINT_LIMIT {
                if let Some(ctx) = &mut self.ctx {
                    let name = entry.file_name();
                    let child_rel = rel_join(rel, &name);
                    let ft = entry.file_type()?;
                    if self.matcher.is_ignored(&child_rel, Some(ft)) {
                        if ft.is_dir() {
                            count_ignored(&entry.path(), &mut ctx.ignored)?;
                        } else {
                            ctx.ignored += 1;
                        }
                    } else if ft.is_dir() {
                        if let Ok(md) = std::fs::symlink_metadata(entry.path()) {
                            add_size(&md, &mut ctx.seen, &mut ctx.size);
                        }
                        count_remaining(&entry.path(), &child_rel, self.matcher, ctx)?;
                    } else {
                        ctx.shown += 1;
                        if let Ok(md) = std::fs::symlink_metadata(entry.path()) {
                            add_size(&md, &mut ctx.seen, &mut ctx.size);
                        }
                    }
                }
                return Ok(true);
            }

            let name = entry.file_name();
            let child_rel = rel_join(rel, &name);
            let ft = entry.file_type()?;
            let is_dir = ft.is_dir();
            let is_symlink = ft.is_symlink();

            if self.matcher.is_ignored(&child_rel, Some(ft)) {
                if let Some(c) = &mut self.ctx {
                    if is_dir {
                        count_ignored(&entry.path(), &mut c.ignored)?;
                    } else {
                        c.ignored += 1;
                    }
                }
                continue;
            }

            if is_dir {
                println!("{}{}/", indent, name.to_string_lossy());
                if let Some(c) = &mut self.ctx {
                    if let Ok(md) = std::fs::symlink_metadata(entry.path()) {
                        add_size(&md, &mut c.seen, &mut c.size);
                    }
                }
                if self.max_depth.is_none_or(|md| depth < md) {
                    if self.list_tree(
                        &entry.path(),
                        &child_rel,
                        &format!("{indent}  "),
                        depth + 1,
                    )? {
                        return Ok(true);
                    }
                } else if let Some(c) = &mut self.ctx {
                    count_remaining(&entry.path(), &child_rel, self.matcher, c)?;
                }
            } else {
                self.printed += 1;
                let suffix = if is_symlink { "@" } else { "" };
                println!("{}{}{}", indent, name.to_string_lossy(), suffix);
                if let Some(c) = &mut self.ctx {
                    c.shown += 1;
                    if let Ok(md) = std::fs::symlink_metadata(entry.path()) {
                        add_size(&md, &mut c.seen, &mut c.size);
                    }
                }
            }
        }

        Ok(false)
    }
}

fn count_remaining(
    abs_dir: &Path,
    rel: &Path,
    matcher: &IgnoreMatcher,
    ctx: &mut ListCtx,
) -> Result<()> {
    for entry in std::fs::read_dir(abs_dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let child_rel = if rel.as_os_str().is_empty() {
            PathBuf::from(name.clone())
        } else {
            rel.join(&name)
        };
        let ft = entry.file_type()?;
        let is_dir = ft.is_dir();

        if matcher.is_ignored(&child_rel, Some(ft)) {
            if is_dir {
                count_ignored(&entry.path(), &mut ctx.ignored)?;
            } else {
                ctx.ignored += 1;
            }
        } else if is_dir {
            if let Ok(md) = std::fs::symlink_metadata(entry.path()) {
                add_size(&md, &mut ctx.seen, &mut ctx.size);
            }
            count_remaining(&entry.path(), &child_rel, matcher, ctx)?;
        } else {
            ctx.shown += 1;
            if let Ok(md) = std::fs::symlink_metadata(entry.path()) {
                add_size(&md, &mut ctx.seen, &mut ctx.size);
            }
        }
    }
    Ok(())
}

fn count_ignored(dir: &Path, ignored: &mut u64) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let ft = entry.file_type()?;
        if ft.is_dir() {
            count_ignored(&entry.path(), ignored)?;
        } else {
            *ignored += 1;
        }
    }
    Ok(())
}

fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}
