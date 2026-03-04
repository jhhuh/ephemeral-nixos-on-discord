/// Integration test: full agent loop with mock QGA server simulating NixOS.
/// Tests that the system prompt + tools + event streaming work correctly.
///
/// Run: cargo test --test mock-qga-agent-test -- --ignored --nocapture
use std::collections::HashMap;

use base64::prelude::*;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;

use ephemeral_nixos_bot::llm::agent::{Agent, AgentEvent};
use ephemeral_nixos_bot::llm::traits::*;
use ephemeral_nixos_bot::qga::QgaClient;

/// Mock LLM that returns predetermined responses based on user input keywords.
struct MockLlm {
    responses: Vec<LlmResponse>,
    call_count: std::sync::atomic::AtomicUsize,
}

impl MockLlm {
    fn new(responses: Vec<LlmResponse>) -> Self {
        Self {
            responses,
            call_count: std::sync::atomic::AtomicUsize::new(0),
        }
    }
}

#[async_trait::async_trait]
impl LlmBackend for MockLlm {
    async fn chat(
        &self,
        _messages: &[Message],
        _tools: &[ToolDef],
    ) -> Result<LlmResponse, Box<dyn std::error::Error + Send + Sync>> {
        let idx = self
            .call_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if idx < self.responses.len() {
            // Clone-ish: we need to reconstruct since LlmResponse isn't Clone
            let resp = &self.responses[idx];
            match resp {
                LlmResponse::Text(t) => Ok(LlmResponse::Text(t.clone())),
                LlmResponse::ToolCalls(calls) => Ok(LlmResponse::ToolCalls(
                    calls
                        .iter()
                        .map(|c| ToolCall {
                            id: c.id.clone(),
                            name: c.name.clone(),
                            input: c.input.clone(),
                        })
                        .collect(),
                )),
            }
        } else {
            Ok(LlmResponse::Text(
                "(mock LLM: no more responses)".into(),
            ))
        }
    }
}

/// Mock QGA server that handles exec, file operations with simulated NixOS output.
async fn mock_nixos_qga(listener: UnixListener) {
    let (stream, _) = listener.accept().await.unwrap();
    let (read, mut write) = tokio::io::split(stream);
    let mut reader = BufReader::new(read);
    let mut files: HashMap<String, Vec<u8>> = HashMap::new();

    // Pre-populate some NixOS files
    files.insert(
        "/etc/hostname".into(),
        b"test-vm\n".to_vec(),
    );
    files.insert(
        "/etc/os-release".into(),
        b"NAME=NixOS\nVERSION=\"26.05 (Warbler)\"\nID=nixos\n".to_vec(),
    );

    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).await.unwrap() == 0 {
            break; // Client disconnected
        }
        let req: serde_json::Value = serde_json::from_str(&line).unwrap();
        let cmd = req["execute"].as_str().unwrap_or("");

        let response = match cmd {
            "guest-sync" => {
                let id = req["arguments"]["id"].as_u64().unwrap();
                format!("{{\"return\": {id}}}")
            }
            "guest-exec" => {
                format!("{{\"return\": {{\"pid\": 1001}}}}")
            }
            "guest-exec-status" => {
                // Simulate NixOS command outputs
                let stdout = b"NixOS 26.05 (Warbler)\n";
                let encoded = BASE64_STANDARD.encode(stdout);
                format!(
                    "{{\"return\": {{\"exited\": true, \"exitcode\": 0, \"out-data\": \"{encoded}\", \"err-data\": \"\"}}}}"
                )
            }
            "guest-file-open" => {
                let path = req["arguments"]["path"].as_str().unwrap_or("/dev/null");
                let mode = req["arguments"]["mode"].as_str().unwrap_or("r");
                // Use path hash as handle
                let handle = path.len() as u64 + 100;
                if mode == "w" {
                    files.insert(path.to_string(), Vec::new());
                }
                format!("{{\"return\": {handle}}}")
            }
            "guest-file-read" => {
                // Return hostname content for any read
                let content = b"test-vm\n";
                let encoded = BASE64_STANDARD.encode(content);
                let count = content.len();
                format!(
                    "{{\"return\": {{\"count\": {count}, \"buf-b64\": \"{encoded}\", \"eof\": true}}}}"
                )
            }
            "guest-file-write" => {
                let count = req["arguments"]["buf-b64"]
                    .as_str()
                    .map(|b| BASE64_STANDARD.decode(b).unwrap_or_default().len())
                    .unwrap_or(0);
                format!("{{\"return\": {{\"count\": {count}, \"eof\": false}}}}")
            }
            "guest-file-close" => {
                format!("{{\"return\": {{}}}}")
            }
            _ => {
                format!("{{\"error\": {{\"class\": \"GenericError\", \"desc\": \"unknown command: {cmd}\"}}}}")
            }
        };

        write
            .write_all(format!("{response}\n").as_bytes())
            .await
            .unwrap();
        write.flush().await.unwrap();
    }
}

#[tokio::test]
#[ignore]
async fn test_agent_exec_flow() {
    let dir = tempfile::tempdir().unwrap();
    let sock_path = dir.path().join("qga.sock");
    let listener = UnixListener::bind(&sock_path).unwrap();

    let server = tokio::spawn(mock_nixos_qga(listener));

    let mut client = QgaClient::connect(&sock_path).await.unwrap();

    // Mock LLM: first call returns a tool call (exec), second returns explanation text
    let mock = MockLlm::new(vec![
        LlmResponse::ToolCalls(vec![ToolCall {
            id: "call_1".into(),
            name: "exec".into(),
            input: serde_json::json!({"command": "nixos-version"}),
        }]),
        LlmResponse::Text(
            "This is NixOS 26.05 (Warbler). NixOS versions are named after birds — \
             each release gets a new codename. The version number corresponds to the \
             year and month of the release branch (26.05 = May 2026)."
                .into(),
        ),
    ]);

    let mut agent = Agent::new(Box::new(mock));

    let mut events = Vec::new();
    let result = agent
        .handle_message("what version of nixos is this?", &mut client, |event| {
            let desc = match &event {
                AgentEvent::ToolStart { name, detail } => {
                    format!("START[{name}]: {detail}")
                }
                AgentEvent::ToolOutput {
                    name,
                    output,
                    success,
                } => {
                    format!("OUTPUT[{name}] success={success}: {output}")
                }
                AgentEvent::Reply(text) => format!("REPLY: {text}"),
            };
            events.push(desc);
            async {}
        })
        .await
        .unwrap();

    println!("=== Events streamed to Discord ===");
    for (i, event) in events.iter().enumerate() {
        println!("  [{i}] {event}");
    }
    println!("\n=== Final response ===");
    println!("  {result}");

    // Verify: tool start event was emitted
    assert!(
        events.iter().any(|e| e.contains("START[exec]")),
        "Should have a ToolStart event for exec"
    );
    // Verify: tool output event was emitted
    assert!(
        events.iter().any(|e| e.contains("OUTPUT[exec]") && e.contains("success=true")),
        "Should have a successful ToolOutput event"
    );
    // Verify: final response mentions NixOS
    assert!(
        result.contains("NixOS"),
        "Final response should mention NixOS"
    );

    println!("\nAll assertions passed!");

    drop(client);
    server.abort();
}

#[tokio::test]
#[ignore]
async fn test_agent_nixos_rebuild_flow() {
    let dir = tempfile::tempdir().unwrap();
    let sock_path = dir.path().join("qga.sock");
    let listener = UnixListener::bind(&sock_path).unwrap();

    let server = tokio::spawn(mock_nixos_qga(listener));

    let mut client = QgaClient::connect(&sock_path).await.unwrap();

    // Mock LLM: calls nixos_rebuild tool, then explains
    let mock = MockLlm::new(vec![
        LlmResponse::ToolCalls(vec![ToolCall {
            id: "call_1".into(),
            name: "nixos_rebuild".into(),
            input: serde_json::json!({
                "config_nix": "{ pkgs, ... }: { environment.systemPackages = [ pkgs.htop ]; }"
            }),
        }]),
        LlmResponse::Text(
            "I've added htop to your system configuration using `environment.systemPackages`. \
             This is the **declarative** NixOS approach — instead of installing packages \
             imperatively with `nix-env`, we declare what we want in the config and rebuild."
                .into(),
        ),
    ]);

    let mut agent = Agent::new(Box::new(mock));

    let mut events = Vec::new();
    let result = agent
        .handle_message("install htop", &mut client, |event| {
            let desc = match &event {
                AgentEvent::ToolStart { name, detail } => format!("START[{name}]: {detail}"),
                AgentEvent::ToolOutput { name, output, success } => {
                    format!("OUTPUT[{name}] success={success}: {output}")
                }
                AgentEvent::Reply(text) => format!("REPLY: {text}"),
            };
            events.push(desc);
            async {}
        })
        .await
        .unwrap();

    println!("=== Events (install htop) ===");
    for (i, e) in events.iter().enumerate() {
        println!("  [{i}] {e}");
    }
    println!("\n=== Response ===\n  {result}");

    // Verify rebuild tool was called
    assert!(events.iter().any(|e| e.contains("START[nixos_rebuild]")));
    // Verify response teaches declarative approach
    assert!(result.contains("declarative"));

    println!("\nAll assertions passed!");

    drop(client);
    server.abort();
}

#[tokio::test]
#[ignore]
async fn test_agent_read_file_flow() {
    let dir = tempfile::tempdir().unwrap();
    let sock_path = dir.path().join("qga.sock");
    let listener = UnixListener::bind(&sock_path).unwrap();

    let server = tokio::spawn(mock_nixos_qga(listener));

    let mut client = QgaClient::connect(&sock_path).await.unwrap();

    let mock = MockLlm::new(vec![
        LlmResponse::ToolCalls(vec![ToolCall {
            id: "call_1".into(),
            name: "read_file".into(),
            input: serde_json::json!({"path": "/etc/hostname"}),
        }]),
        LlmResponse::Text("The hostname is `test-vm`. On NixOS, this is set declaratively via `networking.hostName` in your configuration.".into()),
    ]);

    let mut agent = Agent::new(Box::new(mock));

    let mut events = Vec::new();
    let result = agent
        .handle_message("what's the hostname?", &mut client, |event| {
            let desc = match &event {
                AgentEvent::ToolStart { name, detail } => format!("START[{name}]: {detail}"),
                AgentEvent::ToolOutput { name, output, success } => {
                    format!("OUTPUT[{name}] success={success}: {output}")
                }
                AgentEvent::Reply(text) => format!("REPLY: {text}"),
            };
            events.push(desc);
            async {}
        })
        .await
        .unwrap();

    println!("=== Events (hostname) ===");
    for (i, e) in events.iter().enumerate() {
        println!("  [{i}] {e}");
    }
    println!("\n=== Response ===\n  {result}");

    assert!(events.iter().any(|e| e.contains("START[read_file]")));
    assert!(events.iter().any(|e| e.contains("test-vm")));
    assert!(result.contains("test-vm"));

    println!("\nAll assertions passed!");

    drop(client);
    server.abort();
}
