use crate::error::{Result, WorktreeError};
use colored::Colorize;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct EnvFileCopier {
    source_repo: PathBuf,
}

impl EnvFileCopier {
    pub fn new(source_repo: &Path) -> Self {
        Self {
            source_repo: source_repo.to_path_buf(),
        }
    }

    pub fn copy_env_files(&self, target_worktree: &Path) -> Result<()> {
        self.copy_env_files_impl(target_worktree, false)
    }

    pub fn copy_env_files_quiet(&self, target_worktree: &Path) -> Result<()> {
        self.copy_env_files_impl(target_worktree, true)
    }

    fn copy_env_files_impl(&self, target_worktree: &Path, quiet: bool) -> Result<()> {
        if !quiet {
            println!("{}", "Copying .env files...".bright_blue());
        }

        let env_files = self.discover_env_files()?;

        if env_files.is_empty() {
            if !quiet {
                println!("  {}", "No .env files found to copy".dimmed());
            }
            return Ok(());
        }

        let mut copied_count = 0;

        for env_file in &env_files {
            let relative_path = env_file
                .strip_prefix(&self.source_repo)
                .map_err(|e| WorktreeError::Other(format!("Failed to compute relative path: {}", e)))?;

            let target_path = target_worktree.join(relative_path);

            // Create parent directory if needed
            if let Some(parent) = target_path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| WorktreeError::Io(e))?;
            }

            // Copy the file
            fs::copy(env_file, &target_path)
                .map_err(|e| WorktreeError::Io(e))?;

            if !quiet {
                println!("  {} {}", "→".bright_green(), relative_path.display());
            }
            copied_count += 1;
        }

        if !quiet {
            println!(
                "{}",
                format!("Copied {} .env file(s)", copied_count).bright_green()
            );
        }

        Ok(())
    }

    fn discover_env_files(&self) -> Result<Vec<PathBuf>> {
        // Use git to find all ignored files, then filter for .env files
        let output = Command::new("git")
            .args([
                "ls-files",
                "--others",
                "--ignored",
                "--exclude-standard",
            ])
            .current_dir(&self.source_repo)
            .output()
            .map_err(|e| WorktreeError::Git(format!("Failed to list gitignored files: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(WorktreeError::Git(format!(
                "git ls-files failed: {}",
                stderr
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut env_files = Vec::new();

        for line in stdout.lines() {
            let line = line.trim();

            // Filter for .env files
            if line.is_empty() {
                continue;
            }

            let file_name = Path::new(line)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");

            // Match .env files with common patterns
            if file_name == ".env"
                || file_name.starts_with(".env.")
                || file_name.ends_with(".env") {
                let full_path = self.source_repo.join(line);

                // Only include if it's actually a file (not a directory)
                if full_path.is_file() {
                    env_files.push(full_path);
                }
            }
        }

        Ok(env_files)
    }

    pub fn list_env_files(&self) -> Result<Vec<PathBuf>> {
        let env_files = self.discover_env_files()?;

        if env_files.is_empty() {
            println!("{}", "No .env files found in repository".dimmed());
            return Ok(Vec::new());
        }

        println!("{}", "Found .env files:".bright_blue());
        for file in &env_files {
            let relative_path = file
                .strip_prefix(&self.source_repo)
                .unwrap_or(file);
            println!("  {} {}", "→".bright_cyan(), relative_path.display());
        }

        Ok(env_files)
    }
}

// Built-in hook implementation for post-create
pub fn run_copy_env_hook(source_repo: &Path, target_worktree: &Path) -> Result<()> {
    let copier = EnvFileCopier::new(source_repo);
    copier.copy_env_files(target_worktree)
}

// Quiet version for TUI contexts where stdout would corrupt the display
pub fn run_copy_env_hook_quiet(source_repo: &Path, target_worktree: &Path) -> Result<()> {
    let copier = EnvFileCopier::new(source_repo);
    copier.copy_env_files_quiet(target_worktree)
}
