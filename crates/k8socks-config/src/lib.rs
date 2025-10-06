use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use directories::BaseDirs;
use merge::Merge;
use serde::Deserialize;
use thiserror::Error;

/// A custom merge strategy for `Option<T>` fields. It overwrites the destination
/// (`left`) with the source (`right`) only if the source is `Some`.
fn overwrite_if_some<T>(left: &mut Option<T>, right: Option<T>) {
    if right.is_some() {
        *left = right;
    }
}

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Configuration file not found at any of the expected locations")]
    NotFound,
    #[error("Failed to read configuration file: {0}")]
    Io(#[from] std::io::Error),
    #[error("Failed to parse configuration file: {0}")]
    Parse(#[from] serde_json::Error),
}

#[derive(Deserialize, Merge, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PodResources {
    #[merge(strategy = overwrite_if_some)]
    #[serde(default)]
    pub cpu: Option<String>,
    #[merge(strategy = overwrite_if_some)]
    #[serde(default)]
    pub memory: Option<String>,
}

#[derive(Deserialize, Merge, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[merge(strategy = overwrite_if_some)]
    #[serde(default)]
    pub kubeconfig: Option<String>,
    #[merge(strategy = overwrite_if_some)]
    #[serde(default)]
    pub context: Option<String>,
    #[merge(strategy = overwrite_if_some)]
    #[serde(default)]
    pub namespace: Option<String>,
    #[merge(strategy = overwrite_if_some)]
    #[serde(default)]
    pub ssh_public_key_path: Option<String>,
    #[merge(strategy = overwrite_if_some)]
    #[serde(default)]
    pub ssh_username: Option<String>,
    #[merge(strategy = overwrite_if_some)]
    #[serde(default)]
    pub local_socks_port: Option<u16>,
    #[merge(strategy = overwrite_if_some)]
    #[serde(default)]
    pub pod_ttl_seconds: Option<u64>,
    #[merge(strategy = overwrite_if_some)]
    #[serde(default)]
    pub pod_image: Option<String>,
    #[merge(strategy = overwrite_if_some)]
    #[serde(default)]
    pub pod_resources: Option<PodResources>,
    #[merge(strategy = overwrite_if_some)]
    #[serde(default)]
    pub pod_labels: Option<HashMap<String, String>>,
    #[merge(strategy = overwrite_if_some)]
    #[serde(default)]
    pub pod_annotations: Option<HashMap<String, String>>,
    #[merge(strategy = overwrite_if_some)]
    #[serde(default)]
    pub log_level: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            kubeconfig: Some("~/.kube/config".to_string()),
            context: None,
            namespace: Some("default".to_string()),
            ssh_public_key_path: Some("~/.ssh/id_rsa.pub".to_string()),
            ssh_username: Some("k8socks".to_string()),
            local_socks_port: Some(1080),
            pod_ttl_seconds: Some(900),
            pod_image: Some("linuxserver/openssh-server:latest".to_string()),
            pod_resources: Some(PodResources {
                cpu: Some("50m".to_string()),
                memory: Some("64Mi".to_string()),
            }),
            pod_labels: Some([("app".to_string(), "k8socks".to_string())].into()),
            pod_annotations: Some(HashMap::new()),
            log_level: Some("info".to_string()),
        }
    }
}

/// Loads configuration from standard paths.
pub fn load_from_paths() -> Result<Config, ConfigError> {
    let home_dir_path = BaseDirs::new().map(|dirs| {
        dirs.home_dir().join(".k8socks/config.json")
    });

    let current_dir_path = Path::new("./config.json").to_path_buf();

    let paths_to_check = [
        home_dir_path,
        Some(current_dir_path)
    ];

    for path in paths_to_check.iter().flatten() {
        if path.exists() {
            let content = fs::read_to_string(path)?;
            let config: Config = serde_json::from_str(&content)?;
            return Ok(config);
        }
    }

    // If no config file is found, return a config with all `None` values.
    Ok(Config {
        kubeconfig: None, context: None, namespace: None,
        ssh_public_key_path: None, ssh_username: None, local_socks_port: None,
        pod_ttl_seconds: None, pod_image: None, pod_resources: None,
        pod_labels: None, pod_annotations: None, log_level: None,
    })
}

/// Resolves a path that may start with `~/`.
pub fn expand_tilde<P: AsRef<Path>>(path: P) -> Option<PathBuf> {
    let path = path.as_ref();
    if !path.starts_with("~") {
        return Some(path.to_path_buf());
    }

    BaseDirs::new().map(|dirs| {
        dirs.home_dir().join(path.strip_prefix("~").unwrap())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_precedence() {
        // 1. Start with defaults
        let mut final_config = Config::default();
        assert_eq!(final_config.namespace, Some("default".to_string()));
        assert_eq!(final_config.local_socks_port, Some(1080));
        assert_eq!(final_config.context, None);

        // 2. Create a "file" config layer
        let file_config = Config {
            namespace: Some("from-file".to_string()),
            local_socks_port: Some(9999),
            context: Some("file-context".to_string()),
            ..Default::default()
        };

        // Merge file config over defaults
        final_config.merge(file_config);
        assert_eq!(final_config.namespace, Some("from-file".to_string()));
        assert_eq!(final_config.local_socks_port, Some(9999));
        assert_eq!(final_config.context, Some("file-context".to_string()));

        // 3. Create a "CLI" config layer
        let cli_config = Config {
            namespace: Some("from-cli".to_string()),
            local_socks_port: None,
            context: None,
            kubeconfig: Some("/path/from/cli".to_string()),
            ssh_public_key_path: None,
            ssh_username: None,
            pod_ttl_seconds: None,
            pod_image: None,
            pod_resources: None,
            pod_labels: None,
            pod_annotations: None,
            log_level: None,
        };

        // Merge CLI config over the existing config
        final_config.merge(cli_config);

        // Assert final state
        assert_eq!(final_config.namespace, Some("from-cli".to_string()));
        assert_eq!(final_config.local_socks_port, Some(9999));
        assert_eq!(final_config.context, Some("file-context".to_string()));
        assert_eq!(final_config.kubeconfig, Some("/path/from/cli".to_string()));
        assert_eq!(final_config.ssh_username, Some("k8socks".to_string()));
    }
}