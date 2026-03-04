use super::traits::ToolDef;
use serde_json::json;

/// Returns tool definitions for sandbox interaction.
pub fn sandbox_tools() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "exec".into(),
            description: "Execute a shell command in the sandbox".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "Shell command to execute"
                    }
                },
                "required": ["command"]
            }),
        },
        ToolDef {
            name: "read_file".into(),
            description: "Read file contents from the sandbox".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Absolute path of the file to read"
                    }
                },
                "required": ["path"]
            }),
        },
        ToolDef {
            name: "write_file".into(),
            description: "Write file contents in the sandbox".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Absolute path of the file to write"
                    },
                    "content": {
                        "type": "string",
                        "description": "Content to write to the file"
                    }
                },
                "required": ["path", "content"]
            }),
        },
        ToolDef {
            name: "nixos_rebuild".into(),
            description: "Apply a NixOS configuration to the sandbox".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "config_nix": {
                        "type": "string",
                        "description": "NixOS configuration expressed as a Nix expression"
                    }
                },
                "required": ["config_nix"]
            }),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tools_serialize_to_valid_json() {
        let tools = sandbox_tools();
        assert_eq!(tools.len(), 4);
        for tool in &tools {
            let json = serde_json::to_string(tool).expect("tool should serialize to JSON");
            let _: serde_json::Value =
                serde_json::from_str(&json).expect("serialized JSON should parse back");
        }
    }

    #[test]
    fn exec_tool_has_command_param() {
        let tools = sandbox_tools();
        let exec = tools.iter().find(|t| t.name == "exec").expect("exec tool should exist");
        let required = exec.parameters["required"]
            .as_array()
            .expect("required should be an array");
        assert!(required.iter().any(|v| v.as_str() == Some("command")));
    }
}
