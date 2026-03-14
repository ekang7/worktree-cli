use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AgentKind {
    Claude,
    Codex,
}

impl AgentKind {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "claude" => Some(Self::Claude),
            "codex" => Some(Self::Codex),
            _ => None,
        }
    }
}

pub struct AgentConfig {
    pub kind: AgentKind,
    pub skip_permissions: bool,
    pub prompt: Option<String>,
}

impl AgentConfig {
    pub fn binary_name(&self) -> &str {
        match self.kind {
            AgentKind::Claude => "claude",
            AgentKind::Codex => "codex",
        }
    }

    pub fn build_command(&self) -> String {
        let mut parts = vec![self.binary_name().to_string()];

        match self.kind {
            AgentKind::Claude => {
                if self.skip_permissions {
                    parts.push("--dangerously-skip-permissions".to_string());
                }
                if let Some(ref prompt) = self.prompt {
                    // Prompt is a positional arg — interactive mode by default
                    parts.push(format!("'{}'", prompt.replace('\'', "'\\''")));
                }
            }
            AgentKind::Codex => {
                if self.skip_permissions {
                    parts.push("--full-auto".to_string());
                }
                if let Some(ref prompt) = self.prompt {
                    parts.push(format!("'{}'", prompt.replace('\'', "'\\''")));
                }
            }
        }

        parts.join(" ")
    }

    pub fn is_available(&self) -> bool {
        Command::new("which")
            .arg(self.binary_name())
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}
