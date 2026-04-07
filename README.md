# worktree-cli

A Rust CLI tool to manage Git worktrees for monorepos with interactive selection, terminal integration, and customizable hooks.

## Features

- **Interactive Selection**: Browse and select worktrees with arrow keys
- **Split Panes by Default**: Opens new tab with Claude Code (left) + Shell (right)
- **Terminal Integration**: Seamless iTerm2/Terminal.app integration
- **Claude Code Integration**: Launch Claude Code in worktrees automatically
- **Hooks System**: Customize worktree lifecycle with pre/post hooks
- **Environment Setup**: Automatically copy `.env` files to new worktrees
- **Git Operations**: Create, remove, and manage worktrees seamlessly

## Installation

### From Source

```bash
cd worktree-cli
cargo install --path .
```

### System-wide Installation

```bash
cargo build --release
sudo cp target/release/worktree-cli /usr/local/bin/worktree
```

## Usage

### Interactive List (Default)

```bash
# Show interactive menu and open split panes on selection
# Left pane: Claude Code | Right pane: Shell
worktree
worktree list

# Only open shell tab without Claude Code or split panes
worktree list --shell-only
```

### Create New Worktree

```bash
# Create worktree (new branch from main, or existing branch)
worktree new feature-login

# Create from specific base branch
worktree new feature-auth --base develop

# Custom directory name
worktree new feature-ui --dir my-custom-name

# Open terminal with split panes (Claude Code + shell)
worktree new feature-api --terminal

# Don't launch Claude Code
worktree new feature-x --no-launch

# Skip all hooks (no .env copying, etc.)
worktree new feature-y --no-hooks

# Strict mode: fail if branch already exists
worktree new feature-login --new
```

> **Note:** `worktree new <branch>` auto-detects whether the branch exists. If it does, it checks out the existing branch into a new worktree. If it doesn't, it creates a fresh branch from the base. The `--new` flag enforces that the branch must not already exist.

### Remove Worktree

```bash
# Interactive removal
worktree remove

# Remove specific worktree
worktree remove feature-login

# Force removal (with uncommitted changes)
worktree remove feature-login --force
```

### Maintenance

```bash
# Clean up stale worktree entries
worktree prune
```

### Hooks Management

```bash
# List available hooks
worktree hooks list

# Initialize .worktree-hooks/ directory with examples
worktree hooks init

# Manually run a specific hook lifecycle
worktree hooks run post-create --path /path/to/worktree
```

## Hooks System

Hooks are customizable scripts that run at different stages of worktree lifecycle:

- **post-create**: After a worktree is created
- **pre-remove**: Before a worktree is removed
- **post-switch**: After switching to an existing worktree

### Initialize Hooks

```bash
worktree hooks init
```

This creates the following structure:

```
.worktree-hooks/
├── post-create.d/
│   ├── 00-copy-env         # Built-in: copy .env files
│   └── 99-example-custom   # Example custom hook
├── pre-remove.d/
└── post-switch.d/
```

### Built-in Hooks

- **00-copy-env**: Automatically copies all `.env*` files from the source repository to the new worktree

### Creating Custom Hooks

1. Create an executable script in the appropriate lifecycle directory:

```bash
# Example: post-create hook to install dependencies
cat > .worktree-hooks/post-create.d/50-install-deps << 'EOF'
#!/bin/bash
cd "$WORKTREE_PATH"
npm install
EOF

chmod +x .worktree-hooks/post-create.d/50-install-deps
```

2. Hooks run in lexicographic order (00-*, 10-*, 20-*, etc.)

3. Available environment variables in hooks:
   - `$WORKTREE_PATH`: Path to the worktree
   - `$WORKTREE_BRANCH`: Branch name
   - `$SOURCE_REPO`: Path to the source repository

### Example Custom Hooks

**Install Dependencies:**
```bash
#!/bin/bash
echo "Installing dependencies for $WORKTREE_BRANCH..."
cd "$WORKTREE_PATH"
npm install
```

**Database Migration:**
```bash
#!/bin/bash
echo "Running database migrations..."
cd "$WORKTREE_PATH"
npm run migrate
```

**Slack Notification:**
```bash
#!/bin/bash
curl -X POST https://hooks.slack.com/... \
  -d "{\"text\": \"New worktree created: $WORKTREE_BRANCH\"}"
```

## Terminal Integration

The tool automatically detects your terminal type and provides seamless integration:

### iTerm2

- Creates new window with split panes:
  - **Left pane**: Claude Code running
  - **Right pane**: Shell in worktree directory

### Terminal.app

- Creates new window with tabs:
  - **Tab 1**: Claude Code running
  - **Tab 2**: Shell in worktree directory

### Usage

```bash
# Use --terminal flag to enable panes/tabs
worktree new feature-name --terminal
```

## Key Behaviors

- **Without `--new`**: Creates a new branch or checks out an existing one automatically
- **With `--new`**: Strict mode — fails if the branch already exists
- **Default base branch**: `main`
- **Worktree naming**: `<project>-<branch-name>` (e.g., `sheets-agent-feature-login`)
- **Worktree location**: Created as siblings to the source repo (e.g., `../sheets-agent-feature-name/`)

## Configuration

### Prerequisites

- **Git**: Required for worktree operations
- **Claude CLI** (optional): For launching Claude Code
  - Install from: https://claude.com/download

### Environment Variables

The tool respects the following environment variables:

- `TERM_PROGRAM`: Used to detect terminal type (iTerm2, Terminal.app)

## Examples

### Complete Workflow

```bash
# 1. Initialize hooks
worktree hooks init

# 2. Create a new feature worktree
worktree new feature-user-auth

# 3. Work on the feature...

# 4. When done, remove the worktree
worktree remove feature-user-auth

# 5. Clean up stale entries
worktree prune
```

### Custom Hook Workflow

```bash
# Create custom hook to setup environment
cat > .worktree-hooks/post-create.d/60-setup << 'EOF'
#!/bin/bash
echo "Setting up environment for $WORKTREE_BRANCH"
cd "$WORKTREE_PATH"

# Install dependencies
npm install

# Copy custom configs
cp "$SOURCE_REPO/config.local.json" .

# Run initial build
npm run build
EOF

chmod +x .worktree-hooks/post-create.d/60-setup

# Create worktree (hooks run automatically)
worktree new feature-test
```

## Troubleshooting

### Claude CLI Not Found

If you see "Claude CLI not found", install it:

```bash
# Visit https://claude.com/download
# Or use: brew install claude
```

### Hooks Not Running

Check that:
1. Hooks directory exists: `.worktree-hooks/`
2. Hook files are executable: `chmod +x .worktree-hooks/post-create.d/*`
3. Hook files have proper shebang: `#!/bin/bash`

### Terminal Integration Not Working

- Ensure `$TERM_PROGRAM` is set correctly
- For iTerm2: Should be `iTerm.app`
- For Terminal.app: Should be `Apple_Terminal`

## Development

### Building

```bash
cargo build --release
```

### Running Tests

```bash
cargo test
```

### Project Structure

```
worktree-cli/
├── Cargo.toml
├── README.md
└── src/
    ├── main.rs          # Entry point
    ├── lib.rs           # Public API
    ├── cli.rs           # Command definitions
    ├── worktree.rs      # Git operations
    ├── interactive.rs   # TUI selection
    ├── terminal.rs      # Terminal integration
    ├── hooks.rs         # Hook system
    ├── env_files.rs     # .env file copying
    └── error.rs         # Error types
```

## License

MIT

## Author

Project FPA
