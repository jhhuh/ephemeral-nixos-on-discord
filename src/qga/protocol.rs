use serde::{Deserialize, Serialize};

// --- Request envelope ---

#[derive(Debug, Serialize)]
pub struct QgaRequest<T: Serialize> {
    pub execute: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<T>,
}

// --- Response types ---

#[derive(Debug, Deserialize)]
pub struct QgaResponse<T> {
    #[serde(rename = "return")]
    pub result: T,
}

#[derive(Debug, Deserialize)]
pub struct QgaError {
    pub error: QgaErrorDetail,
}

#[derive(Debug, Deserialize)]
pub struct QgaErrorDetail {
    pub class: String,
    pub desc: String,
}

// --- guest-sync ---

#[derive(Debug, Serialize)]
pub struct GuestSyncArgs {
    pub id: u64,
}

// --- guest-exec ---

#[derive(Debug, Serialize)]
pub struct GuestExecArgs {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arg: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<Vec<String>>,
    #[serde(rename = "input-data", skip_serializing_if = "Option::is_none")]
    pub input_data: Option<String>,
    #[serde(rename = "capture-output")]
    pub capture_output: bool,
}

#[derive(Debug, Deserialize)]
pub struct GuestExecResult {
    pub pid: u64,
}

// --- guest-exec-status ---

#[derive(Debug, Serialize)]
pub struct GuestExecStatusArgs {
    pub pid: u64,
}

#[derive(Debug, Deserialize)]
pub struct GuestExecStatusResult {
    pub exited: bool,
    #[serde(default)]
    pub exitcode: Option<i32>,
    #[serde(default)]
    pub signal: Option<i32>,
    #[serde(rename = "out-data", default)]
    pub out_data: Option<String>,
    #[serde(rename = "err-data", default)]
    pub err_data: Option<String>,
    #[serde(rename = "out-truncated", default)]
    pub out_truncated: Option<bool>,
    #[serde(rename = "err-truncated", default)]
    pub err_truncated: Option<bool>,
}

// --- guest-file-open ---

#[derive(Debug, Serialize)]
pub struct GuestFileOpenArgs {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
}

// --- guest-file-read ---

#[derive(Debug, Serialize)]
pub struct GuestFileReadArgs {
    pub handle: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct GuestFileReadResult {
    pub count: u64,
    #[serde(rename = "buf-b64")]
    pub buf_b64: String,
    pub eof: bool,
}

// --- guest-file-write ---

#[derive(Debug, Serialize)]
pub struct GuestFileWriteArgs {
    pub handle: u64,
    #[serde(rename = "buf-b64")]
    pub buf_b64: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct GuestFileWriteResult {
    pub count: u64,
    pub eof: bool,
}

// --- guest-file-close ---

#[derive(Debug, Serialize)]
pub struct GuestFileCloseArgs {
    pub handle: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    #[test]
    fn serialize_guest_exec() {
        let req = QgaRequest {
            execute: "guest-exec",
            arguments: Some(GuestExecArgs {
                path: "/bin/ls".to_string(),
                arg: Some(vec!["-la".to_string()]),
                env: None,
                input_data: None,
                capture_output: true,
            }),
        };
        let v: Value = serde_json::to_value(&req).unwrap();
        assert_eq!(v["execute"], "guest-exec");
        assert_eq!(v["arguments"]["path"], "/bin/ls");
        assert_eq!(v["arguments"]["capture-output"], true);
        assert_eq!(v["arguments"]["arg"], json!(["-la"]));
        // Optional None fields must be omitted
        assert!(v["arguments"].get("env").is_none());
        assert!(v["arguments"].get("input-data").is_none());
    }

    #[test]
    fn deserialize_guest_exec_result() {
        let json_str = r#"{"return": {"pid": 42}}"#;
        let resp: QgaResponse<GuestExecResult> = serde_json::from_str(json_str).unwrap();
        assert_eq!(resp.result.pid, 42);
    }

    #[test]
    fn deserialize_exec_status_with_output() {
        let json_str = r#"{
            "return": {
                "exited": true,
                "exitcode": 0,
                "out-data": "aGVsbG8K",
                "err-data": ""
            }
        }"#;
        let resp: QgaResponse<GuestExecStatusResult> = serde_json::from_str(json_str).unwrap();
        let status = resp.result;
        assert!(status.exited);
        assert_eq!(status.exitcode, Some(0));
        assert_eq!(status.out_data.as_deref(), Some("aGVsbG8K"));
        assert_eq!(status.err_data.as_deref(), Some(""));
        assert_eq!(status.signal, None);
        assert_eq!(status.out_truncated, None);
        assert_eq!(status.err_truncated, None);
    }

    #[test]
    fn deserialize_error_response() {
        let json_str = r#"{"error": {"class": "GenericError", "desc": "command not found"}}"#;
        let resp: QgaError = serde_json::from_str(json_str).unwrap();
        assert_eq!(resp.error.class, "GenericError");
        assert_eq!(resp.error.desc, "command not found");
    }

    #[test]
    fn serialize_file_open() {
        let req = QgaRequest {
            execute: "guest-file-open",
            arguments: Some(GuestFileOpenArgs {
                path: "/etc/hostname".to_string(),
                mode: None,
            }),
        };
        let v: Value = serde_json::to_value(&req).unwrap();
        assert_eq!(v["execute"], "guest-file-open");
        assert_eq!(v["arguments"]["path"], "/etc/hostname");
        // Optional None mode must be omitted
        assert!(v["arguments"].get("mode").is_none());

        // With mode set
        let req2 = QgaRequest {
            execute: "guest-file-open",
            arguments: Some(GuestFileOpenArgs {
                path: "/tmp/out".to_string(),
                mode: Some("w".to_string()),
            }),
        };
        let v2: Value = serde_json::to_value(&req2).unwrap();
        assert_eq!(v2["arguments"]["mode"], "w");
    }

    #[test]
    fn deserialize_file_read() {
        let json_str = r#"{
            "return": {
                "count": 12,
                "buf-b64": "aGVsbG8gd29ybGQK",
                "eof": false
            }
        }"#;
        let resp: QgaResponse<GuestFileReadResult> = serde_json::from_str(json_str).unwrap();
        assert_eq!(resp.result.count, 12);
        assert_eq!(resp.result.buf_b64, "aGVsbG8gd29ybGQK");
        assert!(!resp.result.eof);
    }
}
