use thiserror::Error;

#[derive(Error, Debug)]
pub enum WorktreeError {
    #[error("Git error: {0}")]
    Git(String),

    #[error("Worktree already exists: {0}")]
    AlreadyExists(String),

    #[error("Worktree not found: {0}")]
    NotFound(String),

    #[error("Branch not found: {0}")]
    BranchNotFound(String),

    #[error("Invalid worktree path: {0}")]
    InvalidPath(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Failed to execute command: {0}")]
    CommandFailed(String),

    #[error("Hook error: {0}")]
    Hook(String),

    #[error("Terminal error: {0}")]
    Terminal(String),

    #[error("Not in a git repository")]
    NotInGitRepo,

    #[error("Cancelled by user")]
    Cancelled,

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, WorktreeError>;
