//! Translates `commands/*.md` (written in Claude Code's format) into each
//! agent's command format, so commands keep a single source.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

/// An agent's command format (`commands_format` in targets.toml).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Format {
    /// Copied as-is: YAML frontmatter, `$ARGUMENTS` placeholder.
    #[default]
    Claude,
    /// Markdown without frontmatter (Cursor commands, Windsurf workflows).
    Plain,
    /// `description` + `prompt` TOML, `{{args}}` placeholder (Gemini CLI).
    GeminiToml,
    /// `.prompt.md` with Copilot frontmatter (VS Code).
    CopilotPrompt,
}

/// Where an agent has no argument placeholder, `$ARGUMENTS` becomes this; the
/// text typed after the command is still added to the prompt by the agent.
const ARGS_PROSE: &str = "the argument the user typed after the command";

/// A parsed `commands/<name>.md`.
pub struct Command<'a> {
    /// File name without `.md`: the slash command's name.
    pub name: &'a str,
    pub description: &'a str,
    pub argument_hint: Option<&'a str>,
    /// Everything after the frontmatter.
    pub body: &'a str,
    /// The whole file, for formats that copy it.
    pub raw: &'a str,
}

impl<'a> Command<'a> {
    pub fn parse(file_name: &'a str, raw: &'a str) -> Self {
        let name = file_name.strip_suffix(".md").unwrap_or(file_name);
        let (frontmatter, body) = split_frontmatter(raw);
        Self {
            name,
            description: field(frontmatter, "description").unwrap_or(name),
            argument_hint: field(frontmatter, "argument-hint"),
            body: body.trim_start_matches(['\r', '\n']),
            raw,
        }
    }
}

/// The installed file name for a command in `format`.
pub fn file_name(format: Format, name: &str) -> String {
    match format {
        Format::Claude | Format::Plain => format!("{name}.md"),
        Format::GeminiToml => format!("{name}.toml"),
        Format::CopilotPrompt => format!("{name}.prompt.md"),
    }
}

#[derive(Serialize)]
struct GeminiCommand<'a> {
    description: &'a str,
    prompt: &'a str,
}

/// The command's contents in `format`.
pub fn render<'a>(format: Format, cmd: &Command<'a>) -> Cow<'a, str> {
    match format {
        Format::Claude => Cow::Borrowed(cmd.raw),
        Format::Plain => Cow::Owned(format!(
            "# {}\n\n{}",
            cmd.description,
            spell_out_args(cmd.body)
        )),
        Format::GeminiToml => {
            let prompt = cmd.body.replace("$ARGUMENTS", "{{args}}");
            let toml = toml::to_string(&GeminiCommand {
                description: cmd.description,
                prompt: &prompt,
            });
            // Two plain strings always serialize.
            Cow::Owned(toml.unwrap_or_default())
        }
        Format::CopilotPrompt => {
            let mut out = format!("---\ndescription: {}\n", yaml_string(cmd.description));
            if let Some(hint) = cmd.argument_hint {
                out.push_str("argument-hint: ");
                out.push_str(&yaml_string(hint));
                out.push('\n');
            }
            // Agent mode: the command runs rustgrade and cargo in the terminal.
            out.push_str("agent: agent\n---\n\n");
            out.push_str(&spell_out_args(cmd.body));
            Cow::Owned(out)
        }
    }
}

/// Replaces `$ARGUMENTS` with prose, dropping code backticks around it so the
/// sentence reads as text.
fn spell_out_args(body: &str) -> String {
    body.replace("`$ARGUMENTS`", ARGS_PROSE)
        .replace("$ARGUMENTS", ARGS_PROSE)
}

/// `(frontmatter, body)`; the frontmatter is empty if the file has none.
fn split_frontmatter(src: &str) -> (&str, &str) {
    let Some(rest) = src
        .strip_prefix("---\n")
        .or_else(|| src.strip_prefix("---\r\n"))
    else {
        return ("", src);
    };
    match rest.find("\n---") {
        Some(end) => {
            let after = &rest[end + 4..];
            (
                &rest[..end],
                after
                    .strip_prefix('\n')
                    .or_else(|| after.strip_prefix("\r\n"))
                    .unwrap_or(after),
            )
        }
        None => ("", src),
    }
}

/// A `key: value` line from simple YAML frontmatter, without quotes.
fn field<'a>(frontmatter: &'a str, key: &str) -> Option<&'a str> {
    frontmatter.lines().find_map(|line| {
        let value = line.strip_prefix(key)?.strip_prefix(':')?.trim();
        Some(value.trim_matches('"'))
    })
}

/// A double-quoted YAML scalar, safe for `[path]`, `:` and quotes.
fn yaml_string(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::COMMANDS;

    const SAMPLE: &str = "---\ndescription: Full review: graded\nargument-hint: \"[path]\"\n---\n\nAudit `$ARGUMENTS` now.\n";

    #[test]
    fn parses_frontmatter_and_body() {
        let cmd = Command::parse("audit.md", SAMPLE);
        assert_eq!(cmd.name, "audit");
        assert_eq!(cmd.description, "Full review: graded");
        assert_eq!(cmd.argument_hint, Some("[path]"));
        assert_eq!(cmd.body, "Audit `$ARGUMENTS` now.\n");
    }

    #[test]
    fn claude_is_a_verbatim_copy() {
        let cmd = Command::parse("audit.md", SAMPLE);
        assert_eq!(render(Format::Claude, &cmd), SAMPLE);
        assert_eq!(file_name(Format::Claude, "audit"), "audit.md");
    }

    #[test]
    fn plain_drops_frontmatter_and_spells_out_arguments() {
        let out = render(Format::Plain, &Command::parse("audit.md", SAMPLE));
        assert_eq!(
            out,
            format!("# Full review: graded\n\nAudit {ARGS_PROSE} now.\n")
        );
    }

    #[test]
    fn gemini_toml_round_trips_with_args_placeholder() {
        let out = render(Format::GeminiToml, &Command::parse("audit.md", SAMPLE));
        let parsed: toml::Table = toml::from_str(&out).expect("valid TOML");
        assert_eq!(parsed["description"].as_str(), Some("Full review: graded"));
        assert_eq!(parsed["prompt"].as_str(), Some("Audit `{{args}}` now.\n"));
        assert_eq!(file_name(Format::GeminiToml, "audit"), "audit.toml");
    }

    #[test]
    fn copilot_prompt_has_quoted_frontmatter() {
        let out = render(Format::CopilotPrompt, &Command::parse("audit.md", SAMPLE));
        assert!(out.starts_with(
            "---\ndescription: \"Full review: graded\"\nargument-hint: \"[path]\"\nagent: agent\n---\n\n"
        ), "{out}");
        assert!(!out.contains("$ARGUMENTS"));
        assert_eq!(file_name(Format::CopilotPrompt, "audit"), "audit.prompt.md");
    }

    /// The real commands convert cleanly to every format.
    #[test]
    fn shipped_commands_convert_to_every_format() {
        for asset in COMMANDS {
            let cmd = Command::parse(asset.path, asset.contents);
            assert!(
                cmd.description.len() > 10,
                "{} has a description",
                asset.path
            );
            for format in [Format::Plain, Format::GeminiToml, Format::CopilotPrompt] {
                let out = render(format, &cmd);
                assert!(!out.contains("$ARGUMENTS"), "{} {format:?}", asset.path);
                // Windsurf rejects workflows over 12000 characters.
                assert!(
                    out.chars().count() < 12_000,
                    "{} {format:?} too long",
                    asset.path
                );
            }
            let toml = render(Format::GeminiToml, &cmd);
            assert!(
                toml::from_str::<toml::Table>(&toml).is_ok(),
                "{} TOML",
                asset.path
            );
        }
    }
}
