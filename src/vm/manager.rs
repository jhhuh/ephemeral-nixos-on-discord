use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tempfile::TempDir;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use super::config::{self, VmConfig};
use crate::qga::client::{QgaClient, QgaError};

#[derive(Debug, thiserror::Error)]
pub enum VmError {
    #[error("Config error: {0}")]
    Config(#[from] config::ConfigError),
    #[error("Build failed: {0}")]
    BuildFailed(String),
    #[error("Launch failed: {0}")]
    LaunchFailed(String),
    #[error("QGA connection failed: {0}")]
    QgaFailed(#[from] QgaError),
    #[error("VM not found: {0}")]
    NotFound(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

struct VmInstance {
    process: Child,
    qga_socket: PathBuf,
    _flake_dir: TempDir, // kept alive to prevent cleanup
}

pub struct VmManager {
    project_root: PathBuf,
    state_dir: PathBuf,
    host_cache_url: String,
    instances: Arc<Mutex<HashMap<String, VmInstance>>>,
}

impl VmManager {
    pub fn new(
        project_root: impl Into<PathBuf>,
        state_dir: impl Into<PathBuf>,
        host_cache_url: impl Into<String>,
    ) -> Self {
        Self {
            project_root: project_root.into(),
            state_dir: state_dir.into(),
            host_cache_url: host_cache_url.into(),
            instances: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn create(&self, user_config: Option<String>) -> Result<String, VmError> {
        let vm_id = uuid::Uuid::new_v4().to_string().replace('-', "")[..8].to_string();

        let vm_config = VmConfig {
            vm_id: vm_id.clone(),
            host_cache_url: self.host_cache_url.clone(),
            user_config_nix: user_config,
            ..Default::default()
        };

        // Create state dir for this VM
        let vm_state_dir = self.state_dir.join(&vm_id);
        tokio::fs::create_dir_all(&vm_state_dir).await?;

        // Generate flake
        let (flake_dir, flake_path) =
            config::generate_vm_flake(&vm_config, &self.project_root)?;

        info!(vm_id = %vm_id, flake_dir = %flake_path.display(), "building VM");

        // Build the VM runner
        let build_output = Command::new("nix")
            .args([
                "build",
                &format!(
                    ".#nixosConfigurations.{}.config.microvm.runner.qemu",
                    vm_id
                ),
                "--no-link",
                "--print-out-paths",
            ])
            .current_dir(&flake_path)
            .output()
            .await?;

        if !build_output.status.success() {
            let stderr = String::from_utf8_lossy(&build_output.stderr).to_string();
            return Err(VmError::BuildFailed(stderr));
        }

        let runner_path = String::from_utf8_lossy(&build_output.stdout)
            .trim()
            .to_string();

        if runner_path.is_empty() {
            return Err(VmError::BuildFailed(
                "nix build produced no output path".to_string(),
            ));
        }

        info!(vm_id = %vm_id, runner = %runner_path, "launching VM");

        // Launch the VM
        let run_bin = Path::new(&runner_path).join("bin/microvm-run");
        let process = Command::new(&run_bin)
            .env("MICROVM_STATE_DIR", &vm_state_dir)
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| VmError::LaunchFailed(e.to_string()))?;

        let qga_socket = vm_state_dir.join("qga.sock");

        debug!(vm_id = %vm_id, qga_socket = %qga_socket.display(), "VM launched");

        let instance = VmInstance {
            process,
            qga_socket,
            _flake_dir: flake_dir,
        };

        self.instances.lock().await.insert(vm_id.clone(), instance);

        Ok(vm_id)
    }

    pub async fn connect_qga(
        &self,
        vm_id: &str,
        timeout: Duration,
    ) -> Result<QgaClient, VmError> {
        let socket_path = {
            let instances = self.instances.lock().await;
            let instance = instances
                .get(vm_id)
                .ok_or_else(|| VmError::NotFound(vm_id.to_string()))?;
            instance.qga_socket.clone()
        };

        let deadline = tokio::time::Instant::now() + timeout;

        // Wait for the socket file to appear
        loop {
            if tokio::time::Instant::now() > deadline {
                return Err(VmError::QgaFailed(QgaError::Timeout));
            }
            if socket_path.exists() {
                break;
            }
            debug!(vm_id = %vm_id, "waiting for QGA socket...");
            tokio::time::sleep(Duration::from_secs(1)).await;
        }

        // Retry connection until timeout
        loop {
            if tokio::time::Instant::now() > deadline {
                return Err(VmError::QgaFailed(QgaError::Timeout));
            }
            match QgaClient::connect(&socket_path).await {
                Ok(client) => {
                    info!(vm_id = %vm_id, "QGA connected");
                    return Ok(client);
                }
                Err(e) => {
                    debug!(vm_id = %vm_id, error = %e, "QGA connect retry");
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        }
    }

    pub async fn destroy(&self, vm_id: &str) -> Result<(), VmError> {
        let mut instance = self
            .instances
            .lock()
            .await
            .remove(vm_id)
            .ok_or_else(|| VmError::NotFound(vm_id.to_string()))?;

        info!(vm_id = %vm_id, "destroying VM");

        // Kill the QEMU process
        if let Err(e) = instance.process.kill().await {
            warn!(vm_id = %vm_id, error = %e, "failed to kill QEMU process");
        }
        let _ = instance.process.wait().await;

        // Remove state directory
        let vm_state_dir = self.state_dir.join(vm_id);
        if vm_state_dir.exists() {
            if let Err(e) = tokio::fs::remove_dir_all(&vm_state_dir).await {
                warn!(vm_id = %vm_id, error = %e, "failed to remove state dir");
            }
        }

        Ok(())
    }

    pub async fn list(&self) -> Vec<String> {
        self.instances.lock().await.keys().cloned().collect()
    }
}
