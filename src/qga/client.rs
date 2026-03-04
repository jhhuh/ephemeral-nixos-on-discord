use std::path::Path;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::prelude::*;
use serde::de::DeserializeOwned;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

use super::protocol::{
    self, GuestExecArgs, GuestExecStatusArgs, GuestFileCloseArgs, GuestFileOpenArgs,
    GuestFileReadArgs, GuestFileWriteArgs, GuestSyncArgs, QgaRequest, QgaResponse,
};

#[derive(Debug, thiserror::Error)]
pub enum QgaError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("QGA error: {class}: {desc}")]
    Qga { class: String, desc: String },
    #[error("Command timed out")]
    Timeout,
    #[error("Command failed with exit code {code}: {stderr}")]
    ExecFailed { code: i32, stderr: String },
}

#[derive(Debug)]
pub struct ExecOutput {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

pub struct QgaClient {
    reader: BufReader<tokio::io::ReadHalf<UnixStream>>,
    writer: tokio::io::WriteHalf<UnixStream>,
}

fn rand_id() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos() as u64
}

impl QgaClient {
    pub async fn connect(socket_path: &Path) -> Result<Self, QgaError> {
        let stream = UnixStream::connect(socket_path).await?;
        let mut client = Self::from_stream(stream);
        client.sync().await?;
        Ok(client)
    }

    fn from_stream(stream: UnixStream) -> Self {
        let (read_half, write_half) = tokio::io::split(stream);
        Self {
            reader: BufReader::new(read_half),
            writer: write_half,
        }
    }

    pub async fn sync(&mut self) -> Result<(), QgaError> {
        let id = rand_id();
        let req = QgaRequest {
            execute: "guest-sync",
            arguments: Some(GuestSyncArgs { id }),
        };
        let json = serde_json::to_string(&req)?;
        let raw = self.send_raw(&json).await?;
        let resp_id: u64 = Self::parse_response(&raw)?;
        if resp_id != id {
            return Err(QgaError::Qga {
                class: "SyncError".to_string(),
                desc: format!("expected sync id {id}, got {resp_id}"),
            });
        }
        Ok(())
    }

    pub async fn send_raw(&mut self, json: &str) -> Result<String, QgaError> {
        self.writer.write_all(json.as_bytes()).await?;
        self.writer.write_all(b"\n").await?;
        self.writer.flush().await?;
        let mut line = String::new();
        self.reader.read_line(&mut line).await?;
        Ok(line)
    }

    fn parse_response<T: DeserializeOwned>(raw: &str) -> Result<T, QgaError> {
        // Try parsing as a successful response first
        if let Ok(resp) = serde_json::from_str::<QgaResponse<T>>(raw) {
            return Ok(resp.result);
        }
        // Try parsing as a QGA error
        if let Ok(err) = serde_json::from_str::<protocol::QgaError>(raw) {
            return Err(QgaError::Qga {
                class: err.error.class,
                desc: err.error.desc,
            });
        }
        // Fall back to a JSON parse error by re-attempting the original parse
        let resp: QgaResponse<T> = serde_json::from_str(raw)?;
        Ok(resp.result)
    }

    pub async fn exec(
        &mut self,
        command: &str,
        timeout: Duration,
    ) -> Result<ExecOutput, QgaError> {
        let deadline = Instant::now() + timeout;

        // Send guest-exec
        let req = QgaRequest {
            execute: "guest-exec",
            arguments: Some(GuestExecArgs {
                path: "/bin/sh".to_string(),
                arg: Some(vec!["-c".to_string(), command.to_string()]),
                env: None,
                input_data: None,
                capture_output: true,
            }),
        };
        let json = serde_json::to_string(&req)?;
        let raw = self.send_raw(&json).await?;
        let result: protocol::GuestExecResult = Self::parse_response(&raw)?;
        let pid = result.pid;

        // Poll guest-exec-status until exited
        loop {
            if Instant::now() > deadline {
                return Err(QgaError::Timeout);
            }

            let status_req = QgaRequest {
                execute: "guest-exec-status",
                arguments: Some(GuestExecStatusArgs { pid }),
            };
            let json = serde_json::to_string(&status_req)?;
            let raw = self.send_raw(&json).await?;
            let status: protocol::GuestExecStatusResult = Self::parse_response(&raw)?;

            if status.exited {
                let exit_code = status.exitcode.unwrap_or(-1);
                let stdout = decode_base64_opt(status.out_data.as_deref())?;
                let stderr = decode_base64_opt(status.err_data.as_deref())?;

                if exit_code != 0 {
                    return Err(QgaError::ExecFailed {
                        code: exit_code,
                        stderr: stderr.clone(),
                    });
                }

                return Ok(ExecOutput {
                    exit_code,
                    stdout,
                    stderr,
                });
            }

            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    pub async fn read_file(&mut self, path: &str) -> Result<Vec<u8>, QgaError> {
        // Open file
        let open_req = QgaRequest {
            execute: "guest-file-open",
            arguments: Some(GuestFileOpenArgs {
                path: path.to_string(),
                mode: Some("r".to_string()),
            }),
        };
        let json = serde_json::to_string(&open_req)?;
        let raw = self.send_raw(&json).await?;
        let handle: u64 = Self::parse_response(&raw)?;

        let mut data = Vec::new();

        // Read chunks until EOF
        loop {
            let read_req = QgaRequest {
                execute: "guest-file-read",
                arguments: Some(GuestFileReadArgs {
                    handle,
                    count: Some(65536),
                }),
            };
            let json = serde_json::to_string(&read_req)?;
            let raw = self.send_raw(&json).await?;
            let result: protocol::GuestFileReadResult = Self::parse_response(&raw)?;

            let chunk = BASE64_STANDARD.decode(&result.buf_b64).map_err(|e| {
                QgaError::Qga {
                    class: "Base64Error".to_string(),
                    desc: e.to_string(),
                }
            })?;
            data.extend_from_slice(&chunk);

            if result.eof {
                break;
            }
        }

        // Close file
        let close_req = QgaRequest {
            execute: "guest-file-close",
            arguments: Some(GuestFileCloseArgs { handle }),
        };
        let json = serde_json::to_string(&close_req)?;
        self.send_raw(&json).await?;

        Ok(data)
    }

    pub async fn write_file(&mut self, path: &str, data: &[u8]) -> Result<(), QgaError> {
        // Open file
        let open_req = QgaRequest {
            execute: "guest-file-open",
            arguments: Some(GuestFileOpenArgs {
                path: path.to_string(),
                mode: Some("w".to_string()),
            }),
        };
        let json = serde_json::to_string(&open_req)?;
        let raw = self.send_raw(&json).await?;
        let handle: u64 = Self::parse_response(&raw)?;

        // Write in 65536-byte chunks
        for chunk in data.chunks(65536) {
            let encoded = BASE64_STANDARD.encode(chunk);
            let write_req = QgaRequest {
                execute: "guest-file-write",
                arguments: Some(GuestFileWriteArgs {
                    handle,
                    buf_b64: encoded,
                    count: Some(chunk.len() as u64),
                }),
            };
            let json = serde_json::to_string(&write_req)?;
            self.send_raw(&json).await?;
        }

        // Close file
        let close_req = QgaRequest {
            execute: "guest-file-close",
            arguments: Some(GuestFileCloseArgs { handle }),
        };
        let json = serde_json::to_string(&close_req)?;
        self.send_raw(&json).await?;

        Ok(())
    }
}

fn decode_base64_opt(s: Option<&str>) -> Result<String, QgaError> {
    match s {
        None | Some("") => Ok(String::new()),
        Some(encoded) => {
            let bytes = BASE64_STANDARD.decode(encoded).map_err(|e| QgaError::Qga {
                class: "Base64Error".to_string(),
                desc: e.to_string(),
            })?;
            Ok(String::from_utf8_lossy(&bytes).into_owned())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader as TokioBufReader};
    use tokio::net::UnixListener;

    #[tokio::test]
    async fn test_exec_command() {
        let dir = tempfile::tempdir().unwrap();
        let sock_path = dir.path().join("qga.sock");

        let listener = UnixListener::bind(&sock_path).unwrap();

        // Spawn mock QGA server
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (read_half, mut write_half) = tokio::io::split(stream);
            let mut reader = TokioBufReader::new(read_half);
            let mut line = String::new();

            // 1. Handle guest-sync
            line.clear();
            reader.read_line(&mut line).await.unwrap();
            let req: serde_json::Value = serde_json::from_str(&line).unwrap();
            assert_eq!(req["execute"], "guest-sync");
            let sync_id = req["arguments"]["id"].as_u64().unwrap();
            let resp = format!("{{\"return\": {sync_id}}}\n");
            write_half.write_all(resp.as_bytes()).await.unwrap();
            write_half.flush().await.unwrap();

            // 2. Handle guest-exec
            line.clear();
            reader.read_line(&mut line).await.unwrap();
            let req: serde_json::Value = serde_json::from_str(&line).unwrap();
            assert_eq!(req["execute"], "guest-exec");
            let resp = "{\"return\": {\"pid\": 99}}\n";
            write_half.write_all(resp.as_bytes()).await.unwrap();
            write_half.flush().await.unwrap();

            // 3. Handle guest-exec-status
            line.clear();
            reader.read_line(&mut line).await.unwrap();
            let req: serde_json::Value = serde_json::from_str(&line).unwrap();
            assert_eq!(req["execute"], "guest-exec-status");
            assert_eq!(req["arguments"]["pid"], 99);
            let resp =
                "{\"return\": {\"exited\": true, \"exitcode\": 0, \"out-data\": \"aGVsbG8K\", \"err-data\": \"\"}}\n";
            write_half.write_all(resp.as_bytes()).await.unwrap();
            write_half.flush().await.unwrap();
        });

        // Client connects and runs command
        let mut client = QgaClient::connect(&sock_path).await.unwrap();
        let output = client
            .exec("echo hello", Duration::from_secs(5))
            .await
            .unwrap();

        assert_eq!(output.exit_code, 0);
        assert_eq!(output.stdout, "hello\n");
        assert_eq!(output.stderr, "");

        server.await.unwrap();
    }
}
