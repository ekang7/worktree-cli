use clap::Parser;
use colored::Colorize;
use std::process;
use worktree_cli::cli::{Cli, Commands, HookAction};
use worktree_cli::env_files::run_copy_env_hook;
use worktree_cli::hooks::HookManager;
use worktree_cli::interactive::select_worktree_for_removal;
use worktree_cli::terminal::TerminalManager;
use worktree_cli::worktree::WorktreeManager;
use worktree_cli::{run_tui, Result};

fn main() {
    let cli = Cli::parse();
    let skip_permissions = cli.dangerously_skip_permissions;

    let result = match cli.command {
        Some(Commands::List { shell_only }) => handle_list(!shell_only, skip_permissions),
        Some(Commands::New {
            branch_name,
            new,
            base,
            dir,
            no_launch,
            no_hooks,
            terminal,
        }) => handle_new(
            &branch_name,
            new,
            &base,
            dir.as_deref(),
            no_launch,
            no_hooks,
            terminal,
            skip_permissions,
        ),
        Some(Commands::Remove { target, force }) => handle_remove(target.as_deref(), force),
        Some(Commands::Prune) => handle_prune(),
        Some(Commands::Hooks { action }) => handle_hooks(action),
        None => {
            // Default: interactive list with Claude Code launch
            handle_list(true, skip_permissions)
        }
    };

    if let Err(e) = result {
        eprintln!("{} {}", "Error:".bright_red().bold(), e);
        process::exit(1);
    }
}

fn handle_list(_launch: bool, skip_permissions: bool) -> Result<()> {
    let manager = WorktreeManager::new()?;

    // Run the TUI - it handles worktree selection/creation internally
    run_tui(&manager, skip_permissions)?;

    Ok(())
}

fn handle_new(
    branch_name: &str,
    new_flag: bool,
    base: &str,
    custom_dir: Option<&str>,
    no_launch: bool,
    no_hooks: bool,
    _terminal: bool, // Deprecated: terminal panes are now default
    skip_permissions: bool,
) -> Result<()> {
    let manager = WorktreeManager::new()?;

    // Create the worktree
    let worktree_path = manager.create_worktree(branch_name, base, custom_dir, new_flag)?;

    // Run post-create hooks unless disabled
    if !no_hooks {
        let hook_manager = HookManager::new(manager.repo_root());

        // Built-in hook: copy .env files
        run_copy_env_hook(manager.repo_root(), &worktree_path)?;

        // Run custom post-create hooks
        hook_manager.run_hooks("post-create", &worktree_path, branch_name)?;
    }

    // Launch terminal/Claude unless disabled
    if !no_launch {
        let terminal_manager = TerminalManager::with_options(skip_permissions);
        // Always use terminal panes - direct spawn doesn't work with Claude's TTY requirements
        terminal_manager.open_terminal_with_panes_for_path(&worktree_path, branch_name)?;
    } else {
        println!("\n{}", "Worktree created successfully!".bright_green());
        println!("To open it, run:");
        println!("  cd {}", worktree_path.display());
    }

    Ok(())
}

fn handle_remove(target: Option<&str>, force: bool) -> Result<()> {
    let manager = WorktreeManager::new()?;
    let worktrees = manager.list_worktrees()?;

    let worktree = if let Some(target_name) = target {
        // Find worktree by branch name or path
        worktrees
            .iter()
            .find(|wt| wt.branch == target_name || wt.path.to_string_lossy().contains(target_name))
            .ok_or_else(|| {
                worktree_cli::WorktreeError::NotFound(format!(
                    "Worktree '{}' not found",
                    target_name
                ))
            })?
    } else {
        // Interactive selection
        select_worktree_for_removal(&worktrees)?
            .ok_or(worktree_cli::WorktreeError::Cancelled)?
    };

    // Run pre-remove hooks
    let hook_manager = HookManager::new(manager.repo_root());
    hook_manager.run_hooks("pre-remove", &worktree.path, &worktree.branch)?;

    // Remove the worktree
    manager.remove_worktree(&worktree.path, force)?;

    Ok(())
}

fn handle_prune() -> Result<()> {
    let manager = WorktreeManager::new()?;

    println!("{}", "Pruning stale worktree entries...".bright_blue());
    manager.git_prune_worktrees()?;
    println!("{}", "Pruning complete!".bright_green());

    Ok(())
}

fn handle_hooks(action: HookAction) -> Result<()> {
    let manager = WorktreeManager::new()?;
    let hook_manager = HookManager::new(manager.repo_root());

    match action {
        HookAction::List => {
            hook_manager.list_hooks()?;
        }
        HookAction::Init => {
            hook_manager.init_hooks_dir()?;
        }
        HookAction::Run { lifecycle, path } => {
            let worktree_path = if let Some(p) = path {
                std::path::PathBuf::from(p)
            } else {
                std::env::current_dir()
                    .map_err(|e| worktree_cli::WorktreeError::Io(e))?
            };

            // Try to determine the branch name
            let branch_name = std::process::Command::new("git")
                .args(["branch", "--show-current"])
                .current_dir(&worktree_path)
                .output()
                .ok()
                .and_then(|output| {
                    if output.status.success() {
                        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
                    } else {
                        None
                    }
                })
                .unwrap_or_else(|| "unknown".to_string());

            println!(
                "{}",
                format!(
                    "Running {} hooks for {}...",
                    lifecycle,
                    worktree_path.display()
                )
                .bright_blue()
            );

            hook_manager.run_hooks(&lifecycle, &worktree_path, &branch_name)?;
        }
    }

    Ok(())
}
