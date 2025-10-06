use std::fs;
use std::path::{Path, PathBuf};
use directories::BaseDirs;
use k8socks_traits::config::{Config, ConfigError, ConfigService};

pub struct ConfigServiceImpl;

impl ConfigService for ConfigServiceImpl {
    fn load_from_paths() -> Result<Config, ConfigError> {
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

    fn expand_tilde<P: AsRef<Path>>(path: P) -> Option<PathBuf> {
        let path = path.as_ref();
        if !path.starts_with("~") {
            return Some(path.to_path_buf());
        }

        BaseDirs::new().map(|dirs| {
            dirs.home_dir().join(path.strip_prefix("~").unwrap())
        })
    }
}

#[cfg(test)]
mod tests {
    use k8socks_traits::config::Config;
    use merge::Merge;

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
            // Fill in the rest with default values to satisfy the struct initialization
            kubeconfig: None,
            ssh_public_key_path: None,
            ssh_username: None,
            pod_ttl_seconds: None,
            pod_image: None,
            pod_resources: None,
            pod_labels: None,
            pod_annotations: None,
            log_level: None,
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