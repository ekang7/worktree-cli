use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "worktree-cli")]
#[command(author, version, about, long_about = None)]
#[command(
    about = "A CLI tool to manage Git worktrees with interactive selection and terminal integration"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Skip Claude Code permission prompts (use with caution)
    #[arg(long, global = true)]
    pub dangerously_skip_permissions: bool,
}

#[derive(Subcommand)]
pub enum Commands {
    /// List and interactively select from existing worktrees (default)
    List {
        /// Only open shell tab (don't launch Claude Code)
        #[arg(long)]
        shell_only: bool,
    },

    /// Create a new worktree
    New {
        /// Branch name for the new worktree
        branch_name: String,

        /// Create new worktree (fails if exists)
        #[arg(long)]
        new: bool,

        /// Base branch to create from (default: main)
        #[arg(long, default_value = "main")]
        base: String,

        /// Custom directory name for worktree
        #[arg(long)]
        dir: Option<String>,

        /// Don't launch Claude Code after creation
        #[arg(long)]
        no_launch: bool,

        /// Skip all hooks (no .env copying, etc.)
        #[arg(long)]
        no_hooks: bool,

        /// [Deprecated] Terminal panes are now the default
        #[arg(long, hide = true)]
        terminal: bool,
    },

    /// Remove a worktree
    Remove {
        /// Specific worktree to remove (interactive if not provided)
        target: Option<String>,

        /// Force removal even with uncommitted changes
        #[arg(long)]
        force: bool,
    },

    /// Clean up stale worktree entries
    Prune,

    /// Manage worktree hooks
    Hooks {
        #[command(subcommand)]
        action: HookAction,
    },
}

#[derive(Subcommand)]
pub enum HookAction {
    /// List available hooks
    List,

    /// Initialize .worktree-hooks/ directory with examples
    Init,

    /// Manually run a specific lifecycle hook
    Run {
        /// Lifecycle to run (post-create, pre-remove, post-switch)
        lifecycle: String,

        /// Target worktree path (current directory if not specified)
        #[arg(long)]
        path: Option<String>,
    },
}
