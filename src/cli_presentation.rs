//! Terminal-only presentation; machine reports are never passed through this layer.
use std::io::IsTerminal;

use crate::EngineError;

#[derive(Clone, Copy, Default)]
pub(crate) struct Style {
    enabled: bool,
}

impl Style {
    fn paint(self, text: &str, rgb: &str) -> String {
        if self.enabled {
            format!("\x1b[38;2;{rgb}m{text}\x1b[0m")
        } else {
            text.to_owned()
        }
    }

    pub(crate) fn help(self, text: &str) -> String {
        text.lines()
            .map(|line| {
                let trimmed = line.trim_start();
                if trimmed.starts_with("Tokito CLI") {
                    self.paint(line, "8;201;170")
                } else if trimmed.ends_with(':') && !trimmed.starts_with("USAGE: tokito") {
                    self.paint(line, "84;86;240")
                } else if trimmed.starts_with("tokito-cli ") {
                    line.replacen("tokito-cli", &self.paint("tokito-cli", "8;201;170"), 1)
                } else {
                    line.to_owned()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub(crate) fn status(self, status: &str) -> String {
        let rgb = match status {
            "passed" => "62;207;142",
            "failed" => "239;92;104",
            _ => "224;164;63",
        };
        self.paint(status, rgb)
    }
}

/// Leading presentation flags leave existing command argument positions intact.
pub(crate) fn parse(args: &[String]) -> Result<(Style, &[String]), EngineError> {
    let mut index = 0;
    let mut mode = None;
    while let Some(option) = args.get(index).map(String::as_str) {
        if option != "--color" && option != "--no-color" {
            break;
        }
        if mode.is_some() {
            return Err(EngineError::new("colour mode was supplied more than once"));
        }
        if option == "--no-color" {
            mode = Some("never");
            index += 1;
        } else {
            let value = args
                .get(index + 1)
                .map(String::as_str)
                .filter(|value| matches!(*value, "auto" | "always" | "never"))
                .ok_or_else(|| {
                    EngineError::new("--color requires auto, always, or never before the command")
                })?;
            mode = Some(value);
            index += 2;
        }
    }
    let no_color = std::env::var_os("NO_COLOR").is_some_and(|value| !value.is_empty());
    let true_color = std::env::var("COLORTERM")
        .is_ok_and(|value| matches!(value.as_str(), "truecolor" | "24bit"))
        || std::env::var_os("WT_SESSION").is_some_and(|value| !value.is_empty());
    let terminal =
        std::io::stdout().is_terminal() && std::env::var("TERM").as_deref() != Ok("dumb");
    let enabled = !no_color
        && match mode.unwrap_or("auto") {
            "always" => true,
            "never" => false,
            _ => terminal && true_color,
        };
    Ok((Style { enabled }, &args[index..]))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn plain_style_preserves_text_and_brand_style_has_balanced_resets() {
        let input = "Tokito CLI 0.1.0\nUSAGE:\n  tokito-cli doctor";
        assert_eq!(Style::default().help(input), input);
        let decorated = Style { enabled: true }.help(input);
        assert!(decorated.contains("\x1b[38;2;8;201;170m"));
        assert_eq!(decorated.matches("\x1b[0m").count(), 3);
    }
}
