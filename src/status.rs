use crate::error::Result;
use serde::Deserialize;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Default)]
pub struct GitStatus {
    pub staged: usize,
    pub unstaged: usize,
    pub untracked: usize,
}

impl GitStatus {
    pub fn is_clean(&self) -> bool {
        self.staged == 0 && self.unstaged == 0 && self.untracked == 0
    }

    pub fn format_display(&self) -> String {
        if self.is_clean() {
            "[clean]".to_string()
        } else {
            let mut parts = Vec::new();
            if self.staged > 0 {
                parts.push(format!("{}+", self.staged));
            }
            if self.unstaged > 0 {
                parts.push(format!("{}~", self.unstaged));
            }
            if self.untracked > 0 {
                parts.push(format!("{}?", self.untracked));
            }
            format!("[{}]", parts.join(" "))
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrState {
    Open,
    Draft,
    Closed,
    Merged,
}

#[derive(Debug, Clone)]
pub struct PrStatus {
    pub number: u32,
    pub state: PrState,
    pub approved: usize,
    pub changes_requested: usize,
    pub ci_passing: Option<bool>,
}

impl PrStatus {
    pub fn format_display(&self) -> String {
        let mut parts = Vec::new();

        // PR number and state
        let pr_label = match self.state {
            PrState::Draft => format!("PR#{} (draft)", self.number),
            PrState::Merged => format!("PR#{} (merged)", self.number),
            PrState::Closed => format!("PR#{} (closed)", self.number),
            PrState::Open => format!("PR#{}", self.number),
        };
        parts.push(pr_label);

        // Review status (only for open/draft PRs)
        if matches!(self.state, PrState::Open | PrState::Draft) {
            if self.approved > 0 {
                parts.push(format!("{}ok", self.approved));
            }
            if self.changes_requested > 0 {
                parts.push(format!("{}x", self.changes_requested));
            }

            // CI status
            if let Some(passing) = self.ci_passing {
                parts.push(if passing {
                    "CI:ok".to_string()
                } else {
                    "CI:x".to_string()
                });
            }
        }

        parts.join(" ")
    }
}

// JSON structures for deserializing gh CLI output
#[derive(Deserialize)]
struct GhPrResponse {
    number: u32,
    state: String,
    #[serde(rename = "isDraft")]
    is_draft: bool,
    reviews: Option<Vec<GhReview>>,
    #[serde(rename = "statusCheckRollup")]
    status_check_rollup: Option<Vec<GhStatusCheck>>,
}

#[derive(Deserialize)]
struct GhReview {
    state: String,
}

#[derive(Deserialize)]
struct GhStatusCheck {
    conclusion: Option<String>,
    status: Option<String>,
}

/// Fetches git status for a worktree path
pub fn fetch_git_status(path: &Path) -> Result<GitStatus> {
    let output = Command::new("git")
        .args(["status", "--porcelain=v1"])
        .current_dir(path)
        .output()
        .map_err(|e| crate::error::WorktreeError::Git(format!("Failed to get git status: {}", e)))?;

    if !output.status.success() {
        return Ok(GitStatus::default());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut status = GitStatus::default();

    for line in stdout.lines() {
        if line.len() < 2 {
            continue;
        }

        let index_status = line.chars().next().unwrap_or(' ');
        let worktree_status = line.chars().nth(1).unwrap_or(' ');

        // '??' means untracked
        if index_status == '?' && worktree_status == '?' {
            status.untracked += 1;
        } else {
            // First column: staged changes (anything except ' ' and '?')
            if index_status != ' ' && index_status != '?' {
                status.staged += 1;
            }
            // Second column: unstaged changes (anything except ' ')
            if worktree_status != ' ' && worktree_status != '?' {
                status.unstaged += 1;
            }
        }
    }

    Ok(status)
}

/// Fetches PR status for a branch using gh CLI
pub fn fetch_pr_status(branch: &str, repo_root: &Path) -> Result<Option<PrStatus>> {
    if !gh_available() {
        return Ok(None);
    }

    // Skip special branches
    if branch.is_empty() || branch == "(bare)" || branch == "(detached)" {
        return Ok(None);
    }

    let output = Command::new("gh")
        .args([
            "pr",
            "view",
            branch,
            "--json",
            "number,state,isDraft,reviews,statusCheckRollup",
        ])
        .current_dir(repo_root)
        .output()
        .map_err(|e| crate::error::WorktreeError::Git(format!("Failed to get PR status: {}", e)))?;

    if !output.status.success() {
        // No PR found for this branch, that's okay
        return Ok(None);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let response: GhPrResponse = match serde_json::from_str(&stdout) {
        Ok(r) => r,
        Err(_) => return Ok(None),
    };

    // Determine PR state
    let state = if response.is_draft {
        PrState::Draft
    } else {
        match response.state.as_str() {
            "OPEN" => PrState::Open,
            "CLOSED" => PrState::Closed,
            "MERGED" => PrState::Merged,
            _ => PrState::Open,
        }
    };

    // Count approvals and changes requested
    let (approved, changes_requested) = if let Some(reviews) = &response.reviews {
        let mut approved = 0usize;
        let mut changes = 0usize;
        for review in reviews {
            match review.state.as_str() {
                "APPROVED" => approved += 1,
                "CHANGES_REQUESTED" => changes += 1,
                _ => {}
            }
        }
        (approved, changes)
    } else {
        (0, 0)
    };

    // Determine CI status
    let ci_passing = if let Some(checks) = &response.status_check_rollup {
        if checks.is_empty() {
            None
        } else {
            let all_success = checks.iter().all(|check| {
                check.conclusion.as_deref() == Some("SUCCESS")
                    || check.status.as_deref() == Some("COMPLETED")
                        && check.conclusion.as_deref() == Some("SUCCESS")
            });
            let any_failure = checks.iter().any(|check| {
                matches!(
                    check.conclusion.as_deref(),
                    Some("FAILURE") | Some("ERROR") | Some("CANCELLED")
                )
            });
            let all_completed = checks
                .iter()
                .all(|check| check.status.as_deref() == Some("COMPLETED"));

            if any_failure {
                Some(false)
            } else if all_success && all_completed {
                Some(true)
            } else {
                // Still pending
                None
            }
        }
    } else {
        None
    };

    Ok(Some(PrStatus {
        number: response.number,
        state,
        approved,
        changes_requested,
        ci_passing,
    }))
}

/// Checks if gh CLI is available
pub fn gh_available() -> bool {
    Command::new("gh")
        .args(["--version"])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Message type for status updates sent from background thread
#[derive(Debug)]
pub enum StatusUpdate {
    GitStatus { index: usize, status: GitStatus },
    PrStatus { index: usize, status: Option<PrStatus> },
}
