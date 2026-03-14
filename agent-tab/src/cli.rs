use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "agent-tab", about = "Manage terminal tabs with AI coding agents")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Open a new agent tab
    Open {
        /// Directory to open in
        path: String,

        /// Tab name (default: directory basename)
        #[arg(long)]
        name: Option<String>,

        /// Agent to use: claude or codex
        #[arg(long, default_value = "claude")]
        agent: String,

        /// Initial prompt for the agent
        #[arg(long)]
        prompt: Option<String>,

        /// Tab color (blue, red, green, cyan, yellow, magenta, orange)
        #[arg(long, default_value = "blue")]
        color: String,

        /// Skip agent permission prompts
        #[arg(long)]
        dangerously_skip_permissions: bool,

        /// Open tab with shell only, no agent
        #[arg(long)]
        no_agent: bool,
    },

    /// Close an agent tab
    Close {
        /// Tab name to close
        name: String,
    },

    /// List open agent tabs
    List,

    /// Print detected terminal environment
    Detect,

    /// Install the agent-tab skill into a repo's .claude/skills/ directory
    InstallSkill {
        /// Target repo path (default: current directory)
        path: Option<String>,
    },

    /// List worktrees with their tab status
    Status {
        /// Repository path (default: current directory)
        #[arg(long)]
        repo: Option<String>,

        /// Default tab color for matching
        #[arg(long, default_value = "blue")]
        color: String,
    },
}
