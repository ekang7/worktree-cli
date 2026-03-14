---
name: agent-tab
description: Manage terminal tabs with AI coding agents (Claude Code, Codex). Open, close, list, detect terminal, and show worktree status.
allowed-tools:
  - Bash(agent-tab:*)
---

# agent-tab

CLI tool for managing terminal tabs with AI coding agents (Claude Code, Codex).

Supported terminals: **iTerm2**, **tmux**, **Terminal.app**

## Commands

### open — Open a new agent tab

```bash
# Open a tab with Claude in a directory
agent-tab open /path/to/project

# Open with a custom tab name and color
agent-tab open ./my-feature --name my-feature --color green

# Open with Codex instead of Claude
agent-tab open . --agent codex

# Open with an initial prompt
agent-tab open . --prompt "Fix the failing tests in src/auth.rs"

# Open with auto-accept permissions (dangerous)
agent-tab open . --dangerously-skip-permissions

# Open a shell-only tab (no agent)
agent-tab open . --name scratch --no-agent
```

**Arguments:**
- `path` (required) — Directory to open in
- `--name` — Tab name (default: directory basename)
- `--agent` — Agent to use: `claude` or `codex` (default: `claude`)
- `--prompt` — Initial prompt for the agent
- `--color` — Tab color: `blue`, `red`, `green`, `cyan`, `yellow`, `magenta`, `orange` (default: `blue`)
- `--dangerously-skip-permissions` — Skip agent permission prompts
- `--no-agent` — Open tab with shell only, no agent

**Tab colors** are applied via iTerm2 escape sequences or tmux styles.

### close — Close an agent tab

```bash
agent-tab close my-feature
```

**Arguments:**
- `name` (required) — Tab name to close

### list — List open agent tabs

```bash
agent-tab list
```

Lists all open tabs detected in the current terminal.

### detect — Print detected terminal environment

```bash
agent-tab detect
```

Prints which terminal type was detected (iTerm2, tmux, Terminal.app, or Unknown).

### status — List worktrees with their tab status

```bash
# Show status for current repo
agent-tab status

# Show status for a specific repo
agent-tab status --repo /path/to/repo

# With custom default color
agent-tab status --color green
```

**Arguments:**
- `--repo` — Repository path (default: current directory)
- `--color` — Default tab color for matching (default: `blue`)

Shows each worktree's branch, git changes (staged/unstaged/untracked), and whether a tab is open for it.

## Common Workflows

### Open tabs for multiple worktrees
```bash
agent-tab open ../feature-auth --color green --prompt "Implement OAuth2 flow"
agent-tab open ../fix-perf --color red --prompt "Profile and fix the slow query in reports.rs"
```

### Check which worktrees have active tabs
```bash
agent-tab status
```

### Clean up all tabs
```bash
agent-tab list
agent-tab close feature-auth
agent-tab close fix-perf
```

## Error Cases
- If the specified agent binary is not installed, the command fails with an error message
- If a tab with the same name already exists, the open command will report a duplicate
- Terminal-specific features (colors, tab naming) degrade gracefully on unsupported terminals
