use std::collections::HashMap;
use std::path::{Path, PathBuf};
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

pub trait ConfigService {
    fn load_from_paths() -> Result<Config, ConfigError>;
    fn expand_tilde<P: AsRef<Path>>(path: P) -> Option<PathBuf>;
}