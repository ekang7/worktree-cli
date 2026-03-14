mod agent;
mod cli;
mod color;
mod terminal;
mod worktree;

use std::path::Path;

use anyhow::Result;
use clap::Parser;
use colored::Colorize;

use crate::agent::{AgentConfig, AgentKind};
use crate::cli::{Cli, Commands};
use crate::color::TabColor;
use crate::terminal::{OpenTabRequest, TerminalManager};
use crate::worktree::ListOptions;

fn main() -> Result<()> {
    let cli = Cli::parse();
    let tm = TerminalManager::detect();

    match cli.command {
        Commands::Open {
            path,
            name,
            agent,
            prompt,
            color,
            dangerously_skip_permissions,
            no_agent,
        } => {
            let abs_path = std::fs::canonicalize(&path)
                .unwrap_or_else(|_| Path::new(&path).to_path_buf());
            let abs_path_str = abs_path.to_string_lossy();

            let tab_name = name.unwrap_or_else(|| {
                abs_path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "agent-tab".to_string())
            });

            let tab_color = TabColor::from_name(&color);

            let agent_cmd = if no_agent {
                None
            } else {
                let kind = AgentKind::from_str(&agent)
                    .ok_or_else(|| anyhow::anyhow!("Unknown agent: '{}'. Use 'claude' or 'codex'.", agent))?;

                let config = AgentConfig {
                    kind,
                    skip_permissions: dangerously_skip_permissions,
                    prompt,
                };

                if !config.is_available() {
                    anyhow::bail!(
                        "'{}' is not installed or not in PATH. Install it first.",
                        config.binary_name()
                    );
                }

                Some(config.build_command())
            };

            println!(
                "{} Opening tab '{}' in {} ({})",
                "→".blue(),
                tab_name.bold(),
                tm.terminal_type(),
                abs_path_str,
            );

            tm.open_tab(&OpenTabRequest {
                path: &abs_path_str,
                tab_name: &tab_name,
                agent_cmd: agent_cmd.as_deref(),
                color: &tab_color,
            })?;

            println!("{} Tab '{}' opened.", "✓".green(), tab_name.bold());
        }

        Commands::Close { name } => {
            println!("{} Closing tab '{}'...", "→".blue(), name.bold());
            let closed = tm.close_tab(&name)?;
            if closed {
                println!("{} Tab '{}' closed.", "✓".green(), name.bold());
            } else {
                println!("{} Tab '{}' not found.", "✗".red(), name.bold());
            }
        }

        Commands::List => {
            let tabs = tm.list_open_tabs();
            if tabs.is_empty() {
                println!("No open tabs found (or listing not supported for {}).", tm.terminal_type());
            } else {
                println!("{} Open tabs in {}:", "→".blue(), tm.terminal_type());
                for tab in &tabs {
                    println!("  • {}", tab);
                }
            }
        }

        Commands::Detect => {
            println!("Detected terminal: {}", tm.terminal_type().to_string().bold());
        }

        Commands::InstallSkill { path } => {
            let target = path
                .map(|p| std::path::PathBuf::from(p))
                .unwrap_or_else(|| std::env::current_dir().expect("Failed to get current directory"));

            let target = std::fs::canonicalize(&target)
                .unwrap_or_else(|_| target.clone());

            let skill_dir = target.join(".claude").join("skills").join("agent-tab");
            std::fs::create_dir_all(&skill_dir)?;

            let skill_content = include_str!("../.claude/skills/agent-tab/SKILL.md");
            let skill_path = skill_dir.join("SKILL.md");
            std::fs::write(&skill_path, skill_content)?;

            println!(
                "{} Installed agent-tab skill to {}",
                "✓".green(),
                skill_path.display()
            );
        }

        Commands::Status { repo, color } => {
            let options = ListOptions {
                repo_path: repo,
                color,
            };
            let worktrees = worktree::list_worktrees(&tm, &options)?;

            if worktrees.is_empty() {
                println!("No worktrees found.");
            } else {
                for wt in &worktrees {
                    let indicator = if wt.is_tab_opened { "●" } else { " " };

                    let status_str = {
                        let gs = &wt.git_status;
                        if gs.staged == 0 && gs.unstaged == 0 && gs.untracked == 0 {
                            "[clean]".to_string()
                        } else {
                            let mut parts = Vec::new();
                            if gs.staged > 0 {
                                parts.push(format!("{}+", gs.staged));
                            }
                            if gs.unstaged > 0 {
                                parts.push(format!("{}~", gs.unstaged));
                            }
                            if gs.untracked > 0 {
                                parts.push(format!("{}?", gs.untracked));
                            }
                            format!("[{}]", parts.join(" "))
                        }
                    };

                    let tab_info = if let Some(name) = &wt.tab_name {
                        format!("{}", name)
                    } else {
                        String::new()
                    };

                    println!(
                        "{} {:<20} {:<12} {:<20} {}",
                        if wt.is_tab_opened {
                            indicator.green().to_string()
                        } else {
                            indicator.to_string()
                        },
                        wt.branch.bold(),
                        status_str,
                        tab_info,
                        wt.path.display(),
                    );
                }
            }
        }
    }

    Ok(())
}
