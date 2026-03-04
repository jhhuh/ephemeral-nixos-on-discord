use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Nix syntax error: {0}")]
    NixSyntax(String),
}

pub struct VmConfig {
    pub vm_id: String,
    pub host_cache_url: String,
    pub user_config_nix: Option<String>,
    pub mem_mb: u32,
    pub vcpu: u32,
}

impl Default for VmConfig {
    fn default() -> Self {
        Self {
            vm_id: String::new(),
            host_cache_url: String::new(),
            user_config_nix: None,
            mem_mb: 1024,
            vcpu: 2,
        }
    }
}

/// Create a temp directory containing a flake.nix that imports microvm.nix and base-vm.nix.
///
/// Returns the TempDir (must be kept alive to prevent cleanup) and the path to the flake directory.
pub fn generate_vm_flake(
    config: &VmConfig,
    project_root: &Path,
) -> Result<(TempDir, PathBuf), ConfigError> {
    let base_vm_path = project_root.join("nix/base-vm.nix");

    let user_module = match &config.user_config_nix {
        Some(expr) => format!("({})", expr),
        None => "({ })".to_string(),
    };

    let flake_content = format!(
        r#"{{
  inputs = {{
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    microvm.url = "github:astro/microvm.nix";
    microvm.inputs.nixpkgs.follows = "nixpkgs";
  }};

  outputs = {{ self, nixpkgs, microvm, ... }}:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${{system}};
    in {{
      nixosConfigurations."{vm_id}" = nixpkgs.lib.nixosSystem {{
        inherit system;
        specialArgs = {{
          vmId = "{vm_id}";
          hostCacheUrl = "{host_cache_url}";
        }};
        modules = [
          microvm.nixosModules.microvm
          {base_vm_path}
          {user_module}
        ];
      }};
    }};
}}"#,
        vm_id = config.vm_id,
        host_cache_url = config.host_cache_url,
        base_vm_path = base_vm_path.display(),
        user_module = user_module,
    );

    let tmp_dir = TempDir::new()?;
    let flake_path = tmp_dir.path().join("flake.nix");
    std::fs::write(&flake_path, &flake_content)?;

    let dir_path = tmp_dir.path().to_path_buf();
    Ok((tmp_dir, dir_path))
}

/// Validate Nix expression syntax by running `nix-instantiate --parse`.
pub fn validate_nix_syntax(nix_expr: &str) -> Result<(), ConfigError> {
    let tmp_dir = TempDir::new()?;
    let tmp_file = tmp_dir.path().join("check.nix");
    std::fs::write(&tmp_file, nix_expr)?;

    let output = Command::new("nix-instantiate")
        .arg("--parse")
        .arg(&tmp_file)
        .output()?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        Err(ConfigError::NixSyntax(stderr))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_flake_writes_file() {
        let project_root = Path::new(env!("CARGO_MANIFEST_DIR"));

        let config = VmConfig {
            vm_id: "abc12345".to_string(),
            host_cache_url: "http://localhost:5557".to_string(),
            user_config_nix: Some(
                "{ pkgs, ... }: { environment.systemPackages = [ pkgs.curl ]; }".to_string(),
            ),
            mem_mb: 1024,
            vcpu: 2,
        };

        let (_tmp_dir, flake_dir) = generate_vm_flake(&config, project_root).unwrap();
        let flake_path = flake_dir.join("flake.nix");

        assert!(flake_path.exists(), "flake.nix should exist");

        let content = std::fs::read_to_string(&flake_path).unwrap();
        assert!(content.contains("abc12345"), "flake should contain vm_id");
        assert!(
            content.contains("http://localhost:5557"),
            "flake should contain host_cache_url"
        );
        assert!(
            content.contains("pkgs.curl"),
            "flake should contain user packages"
        );
        assert!(
            content.contains("microvm.nixosModules.microvm"),
            "flake should import microvm module"
        );
        assert!(
            content.contains("nix/base-vm.nix"),
            "flake should reference base-vm.nix"
        );
    }

    #[test]
    fn test_validate_nix_syntax_valid() {
        // Skip if nix-instantiate is not available
        if Command::new("nix-instantiate").arg("--version").output().is_err() {
            eprintln!("skipping: nix-instantiate not available");
            return;
        }

        let result = validate_nix_syntax("{ pkgs, ... }: { environment.systemPackages = [ pkgs.curl ]; }");
        assert!(result.is_ok(), "valid nix should pass: {:?}", result);
    }

    #[test]
    fn test_validate_nix_syntax_invalid() {
        // Skip if nix-instantiate is not available
        if Command::new("nix-instantiate").arg("--version").output().is_err() {
            eprintln!("skipping: nix-instantiate not available");
            return;
        }

        let result = validate_nix_syntax("{ pkgs ... }: {");
        assert!(result.is_err(), "invalid nix should fail");
        match result.unwrap_err() {
            ConfigError::NixSyntax(msg) => {
                assert!(!msg.is_empty(), "error message should not be empty");
            }
            other => panic!("expected NixSyntax error, got: {:?}", other),
        }
    }
}
