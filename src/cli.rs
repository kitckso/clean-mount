use clap::{Args, Parser, Subcommand};
use clap_complete::Shell;
use std::path::PathBuf;

#[derive(Args, Debug, Clone)]
pub struct CommonOpts {
    /// Allow other users to access the mount.
    /// Usually requires `user_allow_other` in /etc/fuse.conf.
    #[arg(long)]
    pub allow_other: bool,

    /// Allow root to access the mount.
    #[arg(long)]
    pub allow_root: bool,

    /// Let the kernel enforce permission checks.
    /// Recommended when using --allow-other.
    #[arg(long)]
    pub default_permissions: bool,

    /// Entry and attribute TTL in seconds.
    #[arg(long, default_value_t = 1)]
    pub ttl_secs: u64,

    /// Always hide .git files/directories.
    #[arg(long)]
    pub hide_git: bool,

    /// Always hide .gitignore files.
    #[arg(long)]
    pub hide_gitignore: bool,

    /// Ignore file name (default: .gitignore). Override to use a different file,
    /// e.g. .dockerignore or .gitignore.extra.
    #[arg(long, default_value = ".gitignore")]
    pub ignore_file: String,

    /// Copy the temp mount path to clipboard.
    #[arg(long)]
    pub clipboard: bool,
}

#[derive(Parser, Debug)]
#[command(
    name = "clean-mount",
    version,
    about = "Mount a read-only mirror of a directory while hiding files matched by ignore rules (default: .gitignore).",
    long_about = "clean-mount creates a FUSE filesystem that mirrors an existing directory, \
                  filtering out files and directories matched by ignore rules (default: .gitignore). \
                  Use --ignore-file to use a different ignore file (e.g. .dockerignore). \
                  This makes ignored files appear nonexistent to tools like ls, find, zip, tar, \
                  editors, and AI agents.",
    args_conflicts_with_subcommands = true
)]
pub struct Cli {
    /// Real directory to mirror.
    pub source: Option<PathBuf>,

    /// Empty directory where the filtered view will be mounted.
    pub mountpoint: Option<PathBuf>,

    #[command(flatten)]
    pub opts: CommonOpts,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Mount the filtered view (persistent, for interactive use).
    /// With MOUNTPOINT: mount at that directory.
    /// Without: create a temp directory and print its path.
    /// Use --daemon to run in the background.
    Mount {
        source: PathBuf,
        mountpoint: Option<PathBuf>,
        #[command(flatten)]
        opts: CommonOpts,
        /// Run the mount in the background. Requires an explicit mountpoint.
        #[arg(long)]
        daemon: bool,
    },

    /// List active daemon mounts.
    Status,

    /// Unmount a running daemon mount by PID or mountpoint.
    Stop {
        /// PID of the daemon to stop.
        #[arg(long, conflicts_with = "mountpoint")]
        pid: Option<u32>,
        /// Mountpoint to unmount.
        mountpoint: Option<PathBuf>,
    },

    /// Open the filtered view in the file manager.
    Open {
        source: PathBuf,
        #[command(flatten)]
        opts: CommonOpts,
    },

    /// Copy the filtered view to a destination (mount, cp -a, unmount).
    Cp {
        source: PathBuf,
        dest: PathBuf,
        #[command(flatten)]
        opts: CommonOpts,
    },

    /// List the filtered view without mounting (dry-run / preview).
    /// Shows what the filtered view would contain.
    List {
        source: PathBuf,
        #[command(flatten)]
        opts: CommonOpts,
        /// Recursively show the full tree (default: top-level only).
        #[arg(long, short)]
        tree: bool,
        /// Show summary statistics after listing.
        #[arg(long, short)]
        summary: bool,
    },

    /// Execute a command with the filtered view mounted (mount, run, unmount).
    /// Use {MOUNT} in arguments to reference the mount path.
    Exec {
        source: PathBuf,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
        #[command(flatten)]
        opts: CommonOpts,
    },

    /// Create a gzipped tarball of the filtered view.
    Tar {
        source: PathBuf,
        output: PathBuf,
        #[command(flatten)]
        opts: CommonOpts,
    },

    /// Create a zip archive of the filtered view.
    Zip {
        source: PathBuf,
        output: PathBuf,
        #[command(flatten)]
        opts: CommonOpts,
    },

    /// Generate shell completion script.
    /// Run `eval "$(clean-mount complete)"` in your shell rc file.
    Complete {
        /// Shell to generate completions for (auto-detect if omitted).
        shell: Option<Shell>,

        /// Install completions by appending the eval line to your shell rc file.
        #[arg(long)]
        install: bool,
    },
}
