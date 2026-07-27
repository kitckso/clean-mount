use anyhow::{bail, Context, Result};
use clap::Parser;
use fuser::{BackgroundSession, Config, MountOption, SessionACL};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;
use tempfile::TempDir;
use tracing_subscriber::EnvFilter;

mod cli;
mod fs;
mod ignore_matcher;
mod inode;
mod metadata;

use crate::cli::{Cli, Commands, CommonOpts};
use crate::fs::GitignoreMirrorFs;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    if cfg!(target_os = "linux") && !Path::new("/dev/fuse").exists() {
        bail!("/dev/fuse not found. Install fuse3 and ensure the fuse kernel module is available.");
    }

    match cli.command {
        None => {
            let source = cli.source.context("SOURCE is required")?;
            cmd_mount(source, cli.mountpoint, &cli.opts)
        }
        Some(Commands::Mount {
            source,
            mountpoint,
            opts,
        }) => cmd_mount(source, mountpoint, &opts),
        Some(Commands::Cp { source, dest, opts }) => cmd_cp(source, dest, &opts),
        Some(Commands::Exec {
            source,
            command,
            opts,
        }) => cmd_exec(source, &command, &opts),
        Some(Commands::Open { source, opts }) => cmd_open(source, &opts),
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
            child.stdin.take().unwrap().write_all(text.as_bytes())?;
            child.wait()
        });
    match result {
        Ok(status) if status.success() => eprintln!("path copied to clipboard"),
        Ok(status) => eprintln!("warning: xclip exited with {}", status),
        Err(e) => eprintln!("warning: failed to copy to clipboard: {}", e),
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
        SessionACL::RootAndOwner
    }
}

fn build_fs(source: &Path, opts: &CommonOpts) -> Result<GitignoreMirrorFs> {
    GitignoreMirrorFs::new(
        source.to_path_buf(),
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
        MountOption::AutoUnmount,
    ];

    if opts.default_permissions {
        mount_options.push(MountOption::DefaultPermissions);
    }

    let mut config = Config::default();
    config.mount_options = mount_options;
    config.acl = acl_from_opts(opts);
    config
}

fn cmd_mount(source: PathBuf, mountpoint: Option<PathBuf>, opts: &CommonOpts) -> Result<()> {
    let source = validate_source(&source)?;

    match mountpoint {
        Some(mp) => {
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
            if mp.starts_with(&source) || source.starts_with(&mp) {
                bail!("source and mountpoint must not be nested inside each other");
            }
            mount_blocking(source, opts, &mp)
        }
        None => {
            let tmp = create_temp_mountpoint(&source)?;
            println!("{}", tmp.path().display());
            copy_to_clipboard(tmp.path(), opts.clipboard);
            mount_blocking(source, opts, tmp.path())
        }
    }
}

fn mount_blocking(source: PathBuf, opts: &CommonOpts, mountpoint: &Path) -> Result<()> {
    let fs = build_fs(&source, opts)?;
    let config = build_config(&source, opts);

    tracing::info!(
        source = %source.display(),
        mountpoint = %mountpoint.display(),
        ttl_secs = opts.ttl_secs,
        hide_git = opts.hide_git,
        hide_gitignore = opts.hide_gitignore,
        "mounting read-only gitignore-filtered FUSE filesystem"
    );

    fuser::mount(fs, mountpoint, &config).context("FUSE mount failed")?;

    tracing::info!("filesystem unmounted");
    Ok(())
}

fn cmd_cp(source: PathBuf, dest: PathBuf, opts: &CommonOpts) -> Result<()> {
    let source = validate_source(&source)?;

    if dest.exists() {
        bail!("destination already exists: {}", dest.display());
    }

    let mountpoint = create_temp_mountpoint(&source)?;
    let _session = mount_and_spawn(&source, opts, mountpoint.path())?;
    copy_to_clipboard(mountpoint.path(), opts.clipboard);

    tracing::info!(
        source = %source.display(),
        dest = %dest.display(),
        "copying filtered files"
    );

    let status = Command::new("cp")
        .args(["-a", "--"])
        .arg(mountpoint.path())
        .arg(&dest)
        .status()
        .context("failed to execute cp")?;

    drop(_session);

    if !status.success() {
        bail!("cp exited with status: {}", status);
    }

    tracing::info!("copy complete");
    Ok(())
}

fn cmd_exec(source: PathBuf, command: &[String], opts: &CommonOpts) -> Result<()> {
    if command.is_empty() {
        bail!("exec requires a command to run");
    }

    let source = validate_source(&source)?;

    let mountpoint = create_temp_mountpoint(&source)?;
    let _session = mount_and_spawn(&source, opts, mountpoint.path())?;
    copy_to_clipboard(mountpoint.path(), opts.clipboard);

    let mount_path_str = mountpoint.path().to_string_lossy().into_owned();

    let (cmd, args) = command.split_first().unwrap();

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

    drop(_session);

    if !status.success() {
        bail!("command exited with status: {}", status);
    }

    Ok(())
}

fn cmd_open(source: PathBuf, opts: &CommonOpts) -> Result<()> {
    let source = validate_source(&source)?;
    let mountpoint = create_temp_mountpoint(&source)?;
    let mount_path = mountpoint.path().to_path_buf();

    let _session = mount_and_spawn(&source, opts, &mount_path)?;
    copy_to_clipboard(&mount_path, opts.clipboard);

    println!("{}", mount_path.display());

    open_file_manager(&mount_path);

    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
        // Process will exit on SIGINT; auto_unmount cleans up
    }
}

fn open_file_manager(path: &Path) {
    let _ = Command::new("xdg-open").arg(path).status();
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
