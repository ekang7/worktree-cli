use crate::error::{Result, WorktreeError};
use colored::Colorize;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone)]
pub struct Worktree {
    pub path: PathBuf,
    pub branch: String,
    pub commit: String,
    pub is_bare: bool,
}

impl Worktree {
    pub fn display_name(&self) -> String {
        format!(
            "{} - {}",
            self.branch.bright_cyan(),
            self.path.display().to_string().dimmed()
        )
    }
}

pub struct WorktreeManager {
    repo_root: PathBuf,
    project_name: String,
}

impl WorktreeManager {
    pub fn new() -> Result<Self> {
        let repo_root = Self::find_git_root()?;
        let project_name = Self::get_project_name(&repo_root)?;

        Ok(Self {
            repo_root,
            project_name,
        })
    }

    fn find_git_root() -> Result<PathBuf> {
        let output = Command::new("git")
            .args(["rev-parse", "--show-toplevel"])
            .output()
            .map_err(|e| WorktreeError::Git(format!("Failed to find git root: {}", e)))?;

        if !output.status.success() {
            return Err(WorktreeError::NotInGitRepo);
        }

        let path = String::from_utf8_lossy(&output.stdout)
            .trim()
            .to_string();
        Ok(PathBuf::from(path))
    }

    fn get_project_name(repo_root: &Path) -> Result<String> {
        repo_root
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_string())
            .ok_or_else(|| WorktreeError::InvalidPath("Could not determine project name".to_string()))
    }

    pub fn repo_root(&self) -> &Path {
        &self.repo_root
    }

    pub fn list_worktrees(&self) -> Result<Vec<Worktree>> {
        let output = Command::new("git")
            .args(["worktree", "list", "--porcelain"])
            .current_dir(&self.repo_root)
            .output()
            .map_err(|e| WorktreeError::Git(format!("Failed to list worktrees: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(WorktreeError::Git(format!("git worktree list failed: {}", stderr)));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        self.parse_worktrees(&stdout)
    }

    fn parse_worktrees(&self, output: &str) -> Result<Vec<Worktree>> {
        let mut worktrees = Vec::new();
        let mut current_worktree: Option<Worktree> = None;

        for line in output.lines() {
            if line.starts_with("worktree ") {
                if let Some(wt) = current_worktree.take() {
                    worktrees.push(wt);
                }
                let path = line.strip_prefix("worktree ").unwrap().to_string();
                current_worktree = Some(Worktree {
                    path: PathBuf::from(path),
                    branch: String::new(),
                    commit: String::new(),
                    is_bare: false,
                });
            } else if line.starts_with("HEAD ") {
                if let Some(ref mut wt) = current_worktree {
                    wt.commit = line.strip_prefix("HEAD ").unwrap().to_string();
                }
            } else if line.starts_with("branch ") {
                if let Some(ref mut wt) = current_worktree {
                    let branch = line.strip_prefix("branch ").unwrap();
                    wt.branch = branch
                        .strip_prefix("refs/heads/")
                        .unwrap_or(branch)
                        .to_string();
                }
            } else if line.starts_with("bare") {
                if let Some(ref mut wt) = current_worktree {
                    wt.is_bare = true;
                }
            } else if line.starts_with("detached") {
                if let Some(ref mut wt) = current_worktree {
                    wt.branch = format!("(detached)");
                }
            }
        }

        if let Some(wt) = current_worktree {
            worktrees.push(wt);
        }

        Ok(worktrees)
    }

    pub fn create_worktree(
        &self,
        branch_name: &str,
        base_branch: &str,
        custom_dir: Option<&str>,
        new_flag: bool,
    ) -> Result<PathBuf> {
        // Fetch latest changes
        println!("{}", "Fetching latest changes...".bright_blue());
        self.git_fetch()?;

        // Prune stale worktrees
        self.git_prune_worktrees()?;

        // Check if branch exists locally and/or remotely
        let local_exists = self.local_branch_exists(branch_name)?;
        let remote_exists = self.remote_branch_exists(branch_name)?;

        if new_flag && (local_exists || remote_exists) {
            return Err(WorktreeError::AlreadyExists(format!(
                "Branch '{}' already exists. Use without --new flag to switch to existing worktree.",
                branch_name
            )));
        }

        // Determine worktree directory
        let default_dir_name = format!("{}-{}", self.project_name, branch_name);
        let worktree_dir_name = custom_dir.unwrap_or(&default_dir_name);
        let worktree_path = self.repo_root
            .parent()
            .ok_or_else(|| WorktreeError::InvalidPath("Cannot determine parent directory".to_string()))?
            .join(worktree_dir_name);

        // Check if worktree already exists at this path
        if worktree_path.exists() {
            return Err(WorktreeError::AlreadyExists(format!(
                "Directory already exists: {}",
                worktree_path.display()
            )));
        }

        // Create worktree based on branch status (matching bash script logic)
        println!(
            "{}",
            format!("Creating worktree at {}...", worktree_path.display()).bright_blue()
        );

        let path_str = worktree_path.to_str().unwrap();
        let output = if local_exists {
            // Case 1: Branch exists locally
            // git worktree add PATH BRANCH_NAME
            println!(
                "{}",
                format!("Branch '{}' exists locally. Creating worktree from existing branch...", branch_name).dimmed()
            );
            Command::new("git")
                .args(["worktree", "add", path_str, branch_name])
                .current_dir(&self.repo_root)
                .output()
                .map_err(|e| WorktreeError::Git(format!("Failed to create worktree: {}", e)))?
        } else if remote_exists {
            // Case 2: Branch exists only on remote
            // git worktree add PATH -b BRANCH_NAME origin/BRANCH_NAME
            println!(
                "{}",
                format!("Branch '{}' exists remotely. Creating worktree and tracking remote branch...", branch_name).dimmed()
            );
            Command::new("git")
                .args(["worktree", "add", path_str, "-b", branch_name, &format!("origin/{}", branch_name)])
                .current_dir(&self.repo_root)
                .output()
                .map_err(|e| WorktreeError::Git(format!("Failed to create worktree: {}", e)))?
        } else {
            // Case 3: New branch
            // git worktree add PATH -b BRANCH_NAME BASE_BRANCH
            // Verify base branch exists first
            if !self.branch_exists(base_branch)? {
                return Err(WorktreeError::BranchNotFound(format!(
                    "Base branch '{}' not found",
                    base_branch
                )));
            }
            println!(
                "{}",
                format!("Creating new branch '{}' from '{}'...", branch_name, base_branch).dimmed()
            );
            Command::new("git")
                .args(["worktree", "add", path_str, "-b", branch_name, base_branch])
                .current_dir(&self.repo_root)
                .output()
                .map_err(|e| WorktreeError::Git(format!("Failed to create worktree: {}", e)))?
        };

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(WorktreeError::Git(format!("git worktree add failed: {}", stderr)));
        }

        println!("{}", "Worktree created successfully!".bright_green());
        Ok(worktree_path)
    }

    /// Create a worktree without printing status messages (for TUI mode)
    pub fn create_worktree_quiet(
        &self,
        branch_name: &str,
        base_branch: &str,
        custom_dir: Option<&str>,
        new_flag: bool,
    ) -> Result<PathBuf> {
        // Fetch latest changes
        self.git_fetch()?;

        // Prune stale worktrees
        self.git_prune_worktrees()?;

        // Check if branch exists locally and/or remotely
        let local_exists = self.local_branch_exists(branch_name)?;
        let remote_exists = self.remote_branch_exists(branch_name)?;

        if new_flag && (local_exists || remote_exists) {
            return Err(WorktreeError::AlreadyExists(format!(
                "Branch '{}' already exists. Use without --new flag to switch to existing worktree.",
                branch_name
            )));
        }

        // Determine worktree directory
        let default_dir_name = format!("{}-{}", self.project_name, branch_name);
        let worktree_dir_name = custom_dir.unwrap_or(&default_dir_name);
        let worktree_path = self.repo_root
            .parent()
            .ok_or_else(|| WorktreeError::InvalidPath("Cannot determine parent directory".to_string()))?
            .join(worktree_dir_name);

        // Check if worktree already exists at this path
        if worktree_path.exists() {
            return Err(WorktreeError::AlreadyExists(format!(
                "Directory already exists: {}",
                worktree_path.display()
            )));
        }

        let path_str = worktree_path.to_str().unwrap();
        let output = if local_exists {
            Command::new("git")
                .args(["worktree", "add", path_str, branch_name])
                .current_dir(&self.repo_root)
                .output()
                .map_err(|e| WorktreeError::Git(format!("Failed to create worktree: {}", e)))?
        } else if remote_exists {
            Command::new("git")
                .args(["worktree", "add", path_str, "-b", branch_name, &format!("origin/{}", branch_name)])
                .current_dir(&self.repo_root)
                .output()
                .map_err(|e| WorktreeError::Git(format!("Failed to create worktree: {}", e)))?
        } else {
            if !self.branch_exists(base_branch)? {
                return Err(WorktreeError::BranchNotFound(format!(
                    "Base branch '{}' not found",
                    base_branch
                )));
            }
            Command::new("git")
                .args(["worktree", "add", path_str, "-b", branch_name, base_branch])
                .current_dir(&self.repo_root)
                .output()
                .map_err(|e| WorktreeError::Git(format!("Failed to create worktree: {}", e)))?
        };

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(WorktreeError::Git(format!("git worktree add failed: {}", stderr)));
        }

        Ok(worktree_path)
    }

    pub fn remove_worktree(&self, path: &Path, force: bool) -> Result<()> {
        println!(
            "{}",
            format!("Removing worktree at {}...", path.display()).bright_blue()
        );

        let mut args = vec!["worktree", "remove"];

        if force {
            args.push("--force");
        }

        args.push(path.to_str().unwrap());

        let output = Command::new("git")
            .args(&args)
            .current_dir(&self.repo_root)
            .output()
            .map_err(|e| WorktreeError::Git(format!("Failed to remove worktree: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(WorktreeError::Git(format!("git worktree remove failed: {}", stderr)));
        }

        // Clean up the directory if it still exists (git worktree remove only removes git metadata)
        if path.exists() {
            if let Err(e) = fs::remove_dir_all(path) {
                eprintln!(
                    "{}",
                    format!("Warning: Could not remove directory {}: {}", path.display(), e).yellow()
                );
            }
        }

        println!("{}", "Worktree removed successfully!".bright_green());
        Ok(())
    }

    /// Remove a worktree without printing status messages (for TUI mode)
    pub fn remove_worktree_quiet(&self, path: &Path, force: bool) -> Result<()> {
        let mut args = vec!["worktree", "remove"];

        if force {
            args.push("--force");
        }

        args.push(path.to_str().unwrap());

        let output = Command::new("git")
            .args(&args)
            .current_dir(&self.repo_root)
            .output()
            .map_err(|e| WorktreeError::Git(format!("Failed to remove worktree: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(WorktreeError::Git(format!("git worktree remove failed: {}", stderr)));
        }

        // Clean up the directory if it still exists
        if path.exists() {
            let _ = fs::remove_dir_all(path);
        }

        Ok(())
    }

    pub fn git_prune_worktrees(&self) -> Result<()> {
        let output = Command::new("git")
            .args(["worktree", "prune"])
            .current_dir(&self.repo_root)
            .output()
            .map_err(|e| WorktreeError::Git(format!("Failed to prune worktrees: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(WorktreeError::Git(format!("git worktree prune failed: {}", stderr)));
        }

        Ok(())
    }

    fn git_fetch(&self) -> Result<()> {
        let output = Command::new("git")
            .args(["fetch", "--all", "--prune"])
            .current_dir(&self.repo_root)
            .output()
            .map_err(|e| WorktreeError::Git(format!("Failed to fetch: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            eprintln!("{}", format!("Warning: git fetch failed: {}", stderr).yellow());
        }

        Ok(())
    }

    fn local_branch_exists(&self, branch_name: &str) -> Result<bool> {
        let output = Command::new("git")
            .args(["show-ref", "--verify", "--quiet", &format!("refs/heads/{}", branch_name)])
            .current_dir(&self.repo_root)
            .output()
            .map_err(|e| WorktreeError::Git(format!("Failed to check local branch: {}", e)))?;

        Ok(output.status.success())
    }

    fn remote_branch_exists(&self, branch_name: &str) -> Result<bool> {
        let output = Command::new("git")
            .args(["show-ref", "--verify", "--quiet", &format!("refs/remotes/origin/{}", branch_name)])
            .current_dir(&self.repo_root)
            .output()
            .map_err(|e| WorktreeError::Git(format!("Failed to check remote branch: {}", e)))?;

        Ok(output.status.success())
    }

    fn branch_exists(&self, branch_name: &str) -> Result<bool> {
        Ok(self.local_branch_exists(branch_name)? || self.remote_branch_exists(branch_name)?)
    }

    pub fn find_worktree_by_branch(&self, branch_name: &str) -> Result<Option<Worktree>> {
        let worktrees = self.list_worktrees()?;
        Ok(worktrees.into_iter().find(|wt| wt.branch == branch_name))
    }

    pub fn get_default_branch(&self) -> Result<String> {
        // Try to get the default branch from remote HEAD
        let output = Command::new("git")
            .args(["symbolic-ref", "refs/remotes/origin/HEAD", "--short"])
            .current_dir(&self.repo_root)
            .output()
            .map_err(|e| WorktreeError::Git(format!("Failed to get default branch: {}", e)))?;

        if output.status.success() {
            let branch = String::from_utf8_lossy(&output.stdout)
                .trim()
                .strip_prefix("origin/")
                .unwrap_or("main")
                .to_string();
            return Ok(branch);
        }

        // Fallback: try to get from remote info
        let output = Command::new("git")
            .args(["remote", "show", "origin"])
            .current_dir(&self.repo_root)
            .output()
            .map_err(|e| WorktreeError::Git(format!("Failed to query remote: {}", e)))?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if line.contains("HEAD branch:") {
                    if let Some(branch) = line.split(':').nth(1) {
                        return Ok(branch.trim().to_string());
                    }
                }
            }
        }

        // Final fallback: check which common branch exists
        for branch in ["main", "master", "develop"] {
            if self.branch_exists(branch)? {
                return Ok(branch.to_string());
            }
        }

        Ok("main".to_string())
    }

    pub fn list_branches(&self) -> Result<Vec<String>> {
        let output = Command::new("git")
            .args(["branch", "--all", "--format=%(refname:short)"])
            .current_dir(&self.repo_root)
            .output()
            .map_err(|e| WorktreeError::Git(format!("Failed to list branches: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(WorktreeError::Git(format!("git branch failed: {}", stderr)));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut branches: Vec<String> = stdout
            .lines()
            .map(|line| line.trim().to_string())
            .filter(|line| !line.is_empty())
            .map(|branch| {
                // Remove "origin/" prefix for remote branches
                if branch.starts_with("origin/") {
                    branch.strip_prefix("origin/").unwrap().to_string()
                } else {
                    branch
                }
            })
            .collect();

        // Remove duplicates and sort
        branches.sort();
        branches.dedup();

        // Filter out special branches
        branches.retain(|b| b != "HEAD" && !b.contains("->"));

        Ok(branches)
    }
}
