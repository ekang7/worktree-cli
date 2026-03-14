pub struct TabColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl TabColor {
    pub fn from_name(name: &str) -> Self {
        match name.to_lowercase().as_str() {
            "red" => Self { r: 180, g: 60, b: 60 },
            "green" => Self { r: 60, g: 180, b: 80 },
            "cyan" => Self { r: 60, g: 180, b: 180 },
            "yellow" => Self { r: 180, g: 180, b: 60 },
            "magenta" => Self { r: 180, g: 60, b: 180 },
            "orange" => Self { r: 220, g: 120, b: 40 },
            // blue is the default
            _ => Self { r: 60, g: 80, b: 180 },
        }
    }

    pub fn iterm2_escape(&self) -> String {
        format!(
            r#"printf '\033]6;1;bg;red;brightness;{r}\a\033]6;1;bg;green;brightness;{g}\a\033]6;1;bg;blue;brightness;{b}\a'"#,
            r = self.r,
            g = self.g,
            b = self.b,
        )
    }

    pub fn tmux_style(&self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }
}
