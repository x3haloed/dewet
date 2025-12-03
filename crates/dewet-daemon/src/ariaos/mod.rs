//! ARIAOS DSL parser and command handling
//!
//! Parses DSL commands from companion responses and structures them for execution.

use regex::Regex;
use serde::{Deserialize, Serialize};

/// A parsed ARIAOS DSL command
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "app", content = "action")]
pub enum AriaosCommand {
    #[serde(rename = "notes")]
    Notes(NotesAction),
}

/// Actions for the Notes app
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", content = "payload")]
pub enum NotesAction {
    #[serde(rename = "set_content")]
    SetContent(String),
    #[serde(rename = "append")]
    Append(String),
    #[serde(rename = "clear")]
    Clear,
    #[serde(rename = "scroll_up")]
    ScrollUp,
    #[serde(rename = "scroll_down")]
    ScrollDown,
    #[serde(rename = "scroll_to_top")]
    ScrollToTop,
    #[serde(rename = "scroll_to_bottom")]
    ScrollToBottom,
}

/// Parse DSL commands from response text
/// 
/// Looks for patterns like:
/// - `ariaos.apps.notes.set_content("text here")`
/// - `ariaos.apps.notes.append("more text")`
/// - `ariaos.apps.notes.clear()`
pub fn parse_commands(text: &str) -> Vec<AriaosCommand> {
    let mut commands = Vec::new();
    
    // Match ariaos.apps.notes.set_content("...")
    let set_content_re = Regex::new(r#"ariaos\.apps\.notes\.set_content\s*\(\s*"([^"]*)"\s*\)"#).unwrap();
    for cap in set_content_re.captures_iter(text) {
        if let Some(content) = cap.get(1) {
            commands.push(AriaosCommand::Notes(NotesAction::SetContent(
                unescape_string(content.as_str())
            )));
        }
    }
    
    // Match ariaos.apps.notes.append("...")
    let append_re = Regex::new(r#"ariaos\.apps\.notes\.append\s*\(\s*"([^"]*)"\s*\)"#).unwrap();
    for cap in append_re.captures_iter(text) {
        if let Some(content) = cap.get(1) {
            commands.push(AriaosCommand::Notes(NotesAction::Append(
                unescape_string(content.as_str())
            )));
        }
    }
    
    // Match ariaos.apps.notes.clear()
    let clear_re = Regex::new(r#"ariaos\.apps\.notes\.clear\s*\(\s*\)"#).unwrap();
    if clear_re.is_match(text) {
        commands.push(AriaosCommand::Notes(NotesAction::Clear));
    }
    
    // Match scroll commands
    let scroll_up_re = Regex::new(r#"ariaos\.apps\.notes\.scroll_up\s*\(\s*\)"#).unwrap();
    if scroll_up_re.is_match(text) {
        commands.push(AriaosCommand::Notes(NotesAction::ScrollUp));
    }
    
    let scroll_down_re = Regex::new(r#"ariaos\.apps\.notes\.scroll_down\s*\(\s*\)"#).unwrap();
    if scroll_down_re.is_match(text) {
        commands.push(AriaosCommand::Notes(NotesAction::ScrollDown));
    }
    
    let scroll_to_top_re = Regex::new(r#"ariaos\.apps\.notes\.scroll_to_top\s*\(\s*\)"#).unwrap();
    if scroll_to_top_re.is_match(text) {
        commands.push(AriaosCommand::Notes(NotesAction::ScrollToTop));
    }
    
    let scroll_to_bottom_re = Regex::new(r#"ariaos\.apps\.notes\.scroll_to_bottom\s*\(\s*\)"#).unwrap();
    if scroll_to_bottom_re.is_match(text) {
        commands.push(AriaosCommand::Notes(NotesAction::ScrollToBottom));
    }
    
    commands
}

/// Strip DSL commands from text, returning the cleaned response
pub fn strip_commands(text: &str) -> String {
    let patterns = [
        r#"ariaos\.apps\.notes\.set_content\s*\(\s*"[^"]*"\s*\)"#,
        r#"ariaos\.apps\.notes\.append\s*\(\s*"[^"]*"\s*\)"#,
        r#"ariaos\.apps\.notes\.clear\s*\(\s*\)"#,
        r#"ariaos\.apps\.notes\.scroll_up\s*\(\s*\)"#,
        r#"ariaos\.apps\.notes\.scroll_down\s*\(\s*\)"#,
        r#"ariaos\.apps\.notes\.scroll_to_top\s*\(\s*\)"#,
        r#"ariaos\.apps\.notes\.scroll_to_bottom\s*\(\s*\)"#,
    ];
    
    let mut result = text.to_string();
    for pattern in patterns {
        let re = Regex::new(pattern).unwrap();
        result = re.replace_all(&result, "").to_string();
    }
    
    // Clean up any resulting double newlines or trailing whitespace
    let multi_newline = Regex::new(r"\n{3,}").unwrap();
    result = multi_newline.replace_all(&result, "\n\n").to_string();
    result.trim().to_string()
}

/// Unescape common escape sequences in string content
fn unescape_string(s: &str) -> String {
    s.replace("\\n", "\n")
        .replace("\\t", "\t")
        .replace("\\\"", "\"")
        .replace("\\\\", "\\")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_set_content() {
        let text = r#"Let me update my notes: ariaos.apps.notes.set_content("User is debugging DSL")"#;
        let commands = parse_commands(text);
        assert_eq!(commands.len(), 1);
        match &commands[0] {
            AriaosCommand::Notes(NotesAction::SetContent(s)) => {
                assert_eq!(s, "User is debugging DSL");
            }
            _ => panic!("Wrong command type"),
        }
    }

    #[test]
    fn test_parse_append() {
        let text = r#"ariaos.apps.notes.append("New observation")"#;
        let commands = parse_commands(text);
        assert_eq!(commands.len(), 1);
        match &commands[0] {
            AriaosCommand::Notes(NotesAction::Append(s)) => {
                assert_eq!(s, "New observation");
            }
            _ => panic!("Wrong command type"),
        }
    }

    #[test]
    fn test_parse_clear() {
        let text = r#"Clearing notes: ariaos.apps.notes.clear()"#;
        let commands = parse_commands(text);
        assert_eq!(commands.len(), 1);
        matches!(&commands[0], AriaosCommand::Notes(NotesAction::Clear));
    }

    #[test]
    fn test_strip_commands() {
        let text = r#"Hey there! ariaos.apps.notes.append("test") How are you?"#;
        let cleaned = strip_commands(text);
        assert_eq!(cleaned, "Hey there!  How are you?");
    }
}

