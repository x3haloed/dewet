//! ARIAOS tool definitions and command handling
//!
//! Defines tools that companions can call to interact with their ARIAOS interface.
//! Replaces the previous DSL-based approach with structured tool calling.

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::llm::{ToolCall, ToolDefinition};

/// A parsed ARIAOS command (internal representation)
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

/// Get tool definitions for ARIAOS capabilities.
/// These are passed to the LLM so it knows what tools are available.
pub fn ariaos_tools() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition::new(
            "notes_set_content",
            "Replace all content in your personal notes with new text. Use this when you want to completely rewrite your notes.",
            json!({
                "type": "object",
                "properties": {
                    "content": {
                        "type": "string",
                        "description": "The new content for your notes (replaces existing content)"
                    }
                },
                "required": ["content"],
                "additionalProperties": false
            }),
        ),
        ToolDefinition::new(
            "notes_append",
            "Add a new line to your personal notes. Use this to add observations, reminders, or thoughts without erasing existing notes.",
            json!({
                "type": "object",
                "properties": {
                    "content": {
                        "type": "string",
                        "description": "The text to append to your notes"
                    }
                },
                "required": ["content"],
                "additionalProperties": false
            }),
        ),
        ToolDefinition::new(
            "notes_clear",
            "Clear all content from your personal notes. Use sparingly - only when you want a fresh start.",
            json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
        ),
        ToolDefinition::new(
            "notes_scroll_up",
            "Scroll your notes view up to see earlier content.",
            json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
        ),
        ToolDefinition::new(
            "notes_scroll_down",
            "Scroll your notes view down to see later content.",
            json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
        ),
        ToolDefinition::new(
            "notes_scroll_to_top",
            "Scroll to the top of your notes.",
            json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
        ),
        ToolDefinition::new(
            "notes_scroll_to_bottom",
            "Scroll to the bottom of your notes.",
            json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
        ),
    ]
}

/// Convert a tool call from the LLM into an ARIAOS command.
/// Returns None if the tool call is not an ARIAOS tool.
pub fn tool_call_to_command(tool_call: &ToolCall) -> Result<Option<AriaosCommand>> {
    let name = &tool_call.function.name;
    let args: Value = serde_json::from_str(&tool_call.function.arguments)
        .unwrap_or(json!({}));

    let command = match name.as_str() {
        "notes_set_content" => {
            let content = args
                .get("content")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("notes_set_content requires 'content' argument"))?
                .to_string();
            Some(AriaosCommand::Notes(NotesAction::SetContent(content)))
        }
        "notes_append" => {
            let content = args
                .get("content")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("notes_append requires 'content' argument"))?
                .to_string();
            Some(AriaosCommand::Notes(NotesAction::Append(content)))
        }
        "notes_clear" => Some(AriaosCommand::Notes(NotesAction::Clear)),
        "notes_scroll_up" => Some(AriaosCommand::Notes(NotesAction::ScrollUp)),
        "notes_scroll_down" => Some(AriaosCommand::Notes(NotesAction::ScrollDown)),
        "notes_scroll_to_top" => Some(AriaosCommand::Notes(NotesAction::ScrollToTop)),
        "notes_scroll_to_bottom" => Some(AriaosCommand::Notes(NotesAction::ScrollToBottom)),
        _ => None, // Not an ARIAOS tool
    };

    Ok(command)
}

/// Convert multiple tool calls to ARIAOS commands.
/// Filters out non-ARIAOS tools and collects any errors.
pub fn tool_calls_to_commands(tool_calls: &[ToolCall]) -> (Vec<AriaosCommand>, Vec<String>) {
    let mut commands = Vec::new();
    let mut errors = Vec::new();

    for call in tool_calls {
        match tool_call_to_command(call) {
            Ok(Some(cmd)) => commands.push(cmd),
            Ok(None) => {} // Not an ARIAOS tool, skip
            Err(e) => errors.push(format!("{}: {}", call.function.name, e)),
        }
    }

    (commands, errors)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::FunctionCall;

    #[test]
    fn test_tool_call_set_content() {
        let call = ToolCall {
            id: "call_123".to_string(),
            call_type: "function".to_string(),
            function: FunctionCall {
                name: "notes_set_content".to_string(),
                arguments: r#"{"content": "Hello world"}"#.to_string(),
            },
        };

        let result = tool_call_to_command(&call).unwrap();
        assert!(matches!(
            result,
            Some(AriaosCommand::Notes(NotesAction::SetContent(s))) if s == "Hello world"
        ));
    }

    #[test]
    fn test_tool_call_append() {
        let call = ToolCall {
            id: "call_456".to_string(),
            call_type: "function".to_string(),
            function: FunctionCall {
                name: "notes_append".to_string(),
                arguments: r#"{"content": "New observation"}"#.to_string(),
            },
        };

        let result = tool_call_to_command(&call).unwrap();
        assert!(matches!(
            result,
            Some(AriaosCommand::Notes(NotesAction::Append(s))) if s == "New observation"
        ));
    }

    #[test]
    fn test_tool_call_clear() {
        let call = ToolCall {
            id: "call_789".to_string(),
            call_type: "function".to_string(),
            function: FunctionCall {
                name: "notes_clear".to_string(),
                arguments: "{}".to_string(),
            },
        };

        let result = tool_call_to_command(&call).unwrap();
        assert!(matches!(
            result,
            Some(AriaosCommand::Notes(NotesAction::Clear))
        ));
    }

    #[test]
    fn test_unknown_tool() {
        let call = ToolCall {
            id: "call_unknown".to_string(),
            call_type: "function".to_string(),
            function: FunctionCall {
                name: "some_other_tool".to_string(),
                arguments: "{}".to_string(),
            },
        };

        let result = tool_call_to_command(&call).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_tools_definition() {
        let tools = ariaos_tools();
        assert_eq!(tools.len(), 7);

        // Check that all tools have proper structure
        for tool in &tools {
            assert_eq!(tool.tool_type, "function");
            assert!(!tool.function.name.is_empty());
            assert!(!tool.function.description.is_empty());
        }
    }
}
