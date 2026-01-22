use crate::error::{Result, WorktreeError};
use colored::Colorize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct HookManager {
    hooks_dir: PathBuf,
    repo_root: PathBuf,
}

#[derive(Debug, Clone)]
pub struct Hook {
    pub name: String,
    pub path: PathBuf,
    pub lifecycle: String,
}

impl HookManager {
    pub fn new(repo_root: &Path) -> Self {
        let hooks_dir = repo_root.join(".worktree-hooks");
        Self {
            hooks_dir,
            repo_root: repo_root.to_path_buf(),
        }
    }

    pub fn hooks_dir(&self) -> &Path {
        &self.hooks_dir
    }

    pub fn discover_hooks(&self, lifecycle: &str) -> Result<Vec<Hook>> {
        let lifecycle_dir = self.hooks_dir.join(format!("{}.d", lifecycle));

        if !lifecycle_dir.exists() {
            return Ok(Vec::new());
        }

        let mut hooks = Vec::new();

        let entries = fs::read_dir(&lifecycle_dir)
            .map_err(|e| WorktreeError::Hook(format!("Failed to read hooks directory: {}", e)))?;

        for entry in entries {
            let entry = entry
                .map_err(|e| WorktreeError::Hook(format!("Failed to read directory entry: {}", e)))?;
            let path = entry.path();

            // Check if file is executable
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let metadata = fs::metadata(&path)
                    .map_err(|e| WorktreeError::Hook(format!("Failed to read file metadata: {}", e)))?;
                let permissions = metadata.permissions();

                if !metadata.is_file() || (permissions.mode() & 0o111) == 0 {
                    continue;
                }
            }

            #[cfg(not(unix))]
            {
                if !path.is_file() {
                    continue;
                }
            }

            let name = entry
                .file_name()
                .to_string_lossy()
                .to_string();

            hooks.push(Hook {
                name,
                path,
                lifecycle: lifecycle.to_string(),
            });
        }

        // Sort hooks by name (lexicographic order)
        hooks.sort_by(|a, b| a.name.cmp(&b.name));

        Ok(hooks)
    }

    pub fn run_hooks(
        &self,
        lifecycle: &str,
        worktree_path: &Path,
        branch_name: &str,
    ) -> Result<()> {
        let hooks = self.discover_hooks(lifecycle)?;

        if hooks.is_empty() {
            return Ok(());
        }

        println!(
            "{}",
            format!("Running {} hooks...", lifecycle).bright_blue()
        );

        // Prepare environment variables
        let mut env_vars = HashMap::new();
        env_vars.insert("WORKTREE_PATH", worktree_path.to_string_lossy().to_string());
        env_vars.insert("WORKTREE_BRANCH", branch_name.to_string());
        env_vars.insert("SOURCE_REPO", self.repo_root.to_string_lossy().to_string());

        for hook in hooks {
            println!("  {} {}", "→".bright_cyan(), hook.name);

            let output = Command::new(&hook.path)
                .envs(env_vars.iter().map(|(k, v)| (*k, v.as_str())))
                .current_dir(&self.repo_root)
                .output()
                .map_err(|e| WorktreeError::Hook(format!("Failed to execute hook '{}': {}", hook.name, e)))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let stdout = String::from_utf8_lossy(&output.stdout);

                eprintln!("{}", format!("Hook '{}' failed:", hook.name).bright_red());
                if !stdout.is_empty() {
                    eprintln!("stdout: {}", stdout);
                }
                if !stderr.is_empty() {
                    eprintln!("stderr: {}", stderr);
                }

                return Err(WorktreeError::Hook(format!(
                    "Hook '{}' exited with status: {}",
                    hook.name,
                    output.status.code().unwrap_or(-1)
                )));
            }

            // Print hook output if not empty
            let stdout = String::from_utf8_lossy(&output.stdout);
            if !stdout.trim().is_empty() {
                for line in stdout.lines() {
                    println!("    {}", line.dimmed());
                }
            }
        }

        println!("{}", "All hooks completed successfully!".bright_green());
        Ok(())
    }

    /// Run hooks silently (for TUI mode where stdout is captured by the terminal)
    pub fn run_hooks_quiet(
        &self,
        lifecycle: &str,
        worktree_path: &Path,
        branch_name: &str,
    ) -> Result<()> {
        let hooks = self.discover_hooks(lifecycle)?;

        if hooks.is_empty() {
            return Ok(());
        }

        // Prepare environment variables
        let mut env_vars = HashMap::new();
        env_vars.insert("WORKTREE_PATH", worktree_path.to_string_lossy().to_string());
        env_vars.insert("WORKTREE_BRANCH", branch_name.to_string());
        env_vars.insert("SOURCE_REPO", self.repo_root.to_string_lossy().to_string());

        for hook in hooks {
            let output = Command::new(&hook.path)
                .envs(env_vars.iter().map(|(k, v)| (*k, v.as_str())))
                .current_dir(&self.repo_root)
                .output()
                .map_err(|e| WorktreeError::Hook(format!("Failed to execute hook '{}': {}", hook.name, e)))?;

            if !output.status.success() {
                return Err(WorktreeError::Hook(format!(
                    "Hook '{}' exited with status: {}",
                    hook.name,
                    output.status.code().unwrap_or(-1)
                )));
            }
        }

        Ok(())
    }

    pub fn init_hooks_dir(&self) -> Result<()> {
        if self.hooks_dir.exists() {
            return Err(WorktreeError::Hook(format!(
                "Hooks directory already exists: {}",
                self.hooks_dir.display()
            )));
        }

        println!(
            "{}",
            format!("Initializing hooks directory at {}...", self.hooks_dir.display()).bright_blue()
        );

        // Create hooks directory structure
        fs::create_dir_all(&self.hooks_dir)
            .map_err(|e| WorktreeError::Hook(format!("Failed to create hooks directory: {}", e)))?;

        let lifecycles = ["post-create", "pre-remove", "post-switch"];

        for lifecycle in &lifecycles {
            let lifecycle_dir = self.hooks_dir.join(format!("{}.d", lifecycle));
            fs::create_dir_all(&lifecycle_dir)
                .map_err(|e| WorktreeError::Hook(format!("Failed to create lifecycle directory: {}", e)))?;

            println!("  {} {}.d/", "✓".bright_green(), lifecycle);
        }

        // Create example custom hook
        let example_hook_path = self.hooks_dir.join("post-create.d/99-example-custom");
        let example_hook_content = r#"#!/bin/bash
# Example custom hook
# This hook runs after a worktree is created
# Available environment variables:
#   $WORKTREE_PATH - Path to the new worktree
#   $WORKTREE_BRANCH - Branch name of the worktree
#   $SOURCE_REPO - Path to the source repository

echo "Custom hook running for branch: $WORKTREE_BRANCH"
echo "Worktree path: $WORKTREE_PATH"

# Add your custom setup here
# Examples:
# - Install dependencies: cd "$WORKTREE_PATH" && npm install
# - Copy additional config files
# - Run database migrations
# - Send notifications
"#;

        fs::write(&example_hook_path, example_hook_content)
            .map_err(|e| WorktreeError::Hook(format!("Failed to write example hook: {}", e)))?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&example_hook_path)
                .map_err(|e| WorktreeError::Hook(format!("Failed to read hook metadata: {}", e)))?
                .permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&example_hook_path, perms)
                .map_err(|e| WorktreeError::Hook(format!("Failed to set hook permissions: {}", e)))?;
        }

        println!("  {} 99-example-custom (example hook)", "✓".bright_green());

        // Create README
        let readme_path = self.hooks_dir.join("README.md");
        let readme_content = r#"# Worktree Hooks

This directory contains hooks that run at different stages of worktree lifecycle.

## Hook Lifecycles

- **post-create.d/** - Runs after a worktree is created
- **pre-remove.d/** - Runs before a worktree is removed
- **post-switch.d/** - Runs after switching to an existing worktree

## Built-in Hooks

- `00-copy-env` - Automatically copies .env files from the source repo (implemented in Rust)

## Custom Hooks

To create a custom hook:

1. Add an executable script to the appropriate lifecycle directory
2. Name it with a numeric prefix (e.g., `50-my-custom-hook`)
3. Hooks run in lexicographic order (00-*, 10-*, 20-*, etc.)
4. Make it executable: `chmod +x .worktree-hooks/post-create.d/50-my-custom-hook`

## Environment Variables

Hooks receive these environment variables:

- `$WORKTREE_PATH` - Path to the worktree
- `$WORKTREE_BRANCH` - Branch name
- `$SOURCE_REPO` - Path to the source repository

## Example Hook

```bash
#!/bin/bash
echo "Setting up worktree for $WORKTREE_BRANCH"
cd "$WORKTREE_PATH"
npm install
```

## Testing Hooks

Run hooks manually:
```bash
worktree-cli hooks run post-create --path /path/to/worktree
```

List available hooks:
```bash
worktree-cli hooks list
```
"#;

        fs::write(&readme_path, readme_content)
            .map_err(|e| WorktreeError::Hook(format!("Failed to write README: {}", e)))?;

        println!("{}", "Hooks directory initialized successfully!".bright_green());
        println!("\nNext steps:");
        println!("  1. Review the example hook: {}", example_hook_path.display());
        println!("  2. Create your custom hooks in the lifecycle directories");
        println!("  3. Make hooks executable: chmod +x <hook-file>");

        Ok(())
    }

    pub fn list_hooks(&self) -> Result<()> {
        let lifecycles = ["post-create", "pre-remove", "post-switch"];

        println!("{}", "Available Hooks:".bright_blue().bold());
        println!();

        for lifecycle in &lifecycles {
            let hooks = self.discover_hooks(lifecycle)?;

            println!("{}", format!("{}:", lifecycle).bright_cyan());

            if hooks.is_empty() {
                println!("  {}", "(no hooks)".dimmed());
            } else {
                for hook in hooks {
                    println!("  {} {}", "→".bright_green(), hook.name);
                }
            }
            println!();
        }

        Ok(())
    }
}
