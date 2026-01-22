use crate::error::{Result, WorktreeError};
use crate::worktree::Worktree;
use inquire::Select;

pub fn select_worktree(worktrees: &[Worktree]) -> Result<Option<&Worktree>> {
    if worktrees.is_empty() {
        println!("No worktrees found.");
        return Ok(None);
    }

    let options: Vec<String> = worktrees
        .iter()
        .map(|wt| {
            let branch = if wt.branch.is_empty() {
                "(bare)".to_string()
            } else {
                wt.branch.clone()
            };
            format!("{} - {}", branch, wt.path.display())
        })
        .collect();

    let selection = Select::new("Select a worktree:", options)
        .prompt();

    match selection {
        Ok(selected) => {
            let index = worktrees
                .iter()
                .position(|wt| {
                    let branch = if wt.branch.is_empty() {
                        "(bare)".to_string()
                    } else {
                        wt.branch.clone()
                    };
                    format!("{} - {}", branch, wt.path.display()) == selected
                })
                .unwrap();
            Ok(Some(&worktrees[index]))
        }
        Err(inquire::InquireError::OperationCanceled) => {
            Err(WorktreeError::Cancelled)
        }
        Err(e) => Err(WorktreeError::Other(format!("Selection error: {}", e))),
    }
}

pub fn select_worktree_for_removal(worktrees: &[Worktree]) -> Result<Option<&Worktree>> {
    // Filter out bare/main worktrees
    let removable: Vec<&Worktree> = worktrees
        .iter()
        .filter(|wt| !wt.is_bare && !wt.branch.is_empty())
        .collect();

    if removable.is_empty() {
        println!("No removable worktrees found.");
        return Ok(None);
    }

    let options: Vec<String> = removable
        .iter()
        .map(|wt| format!("{} - {}", wt.branch, wt.path.display()))
        .collect();

    let selection = Select::new("Select a worktree to remove:", options)
        .prompt();

    match selection {
        Ok(selected) => {
            let index = removable
                .iter()
                .position(|wt| format!("{} - {}", wt.branch, wt.path.display()) == selected)
                .unwrap();
            Ok(Some(removable[index]))
        }
        Err(inquire::InquireError::OperationCanceled) => {
            Err(WorktreeError::Cancelled)
        }
        Err(e) => Err(WorktreeError::Other(format!("Selection error: {}", e))),
    }
}
