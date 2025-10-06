use std::collections::BTreeMap;
use std::fs;
use std::time::Duration;
use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use k8s_openapi::api::core::v1::{
    Container, Pod, PodSpec, ResourceRequirements,
};
use k8s_openapi::apimachinery::pkg::api::resource::Quantity;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use kube::api::{Api, DeleteParams, PostParams};
use kube::config::{InferConfigError, KubeconfigError};
use kube::runtime::wait::{await_condition, conditions};
use kube::{Client, Config as KubeConfig, Error as KubeError};
use rand::Rng;
use thiserror::Error;
use tokio::io;
use tokio::task::JoinHandle;

use k8socks_config::Config;

#[derive(Error, Debug)]
pub enum K8sError {
    #[error("Kubernetes API error: {0}")]
    Kube(#[from] KubeError),
    #[error("Kubernetes config error: {0}")]
    KubeConfig(#[from] KubeconfigError),
    #[error("Failed to infer Kubernetes config: {0}")]
    InferConfig(#[from] InferConfigError),
    #[error("Pod was not ready in time")]
    PodNotReady,
    #[error("Failed to read SSH public key at '{0}': {1}")]
    SshKeyError(String, std::io::Error),
    #[error("Pod was not found: {0}")]
    PodNotFound(String),
}

#[derive(Clone, Debug)]
pub struct PodRef {
    pub name: String,
    pub namespace: String,
}

pub struct PortForwardHandle {
    pub local_port: u16,
    _handle: JoinHandle<()>,
}

#[async_trait]
pub trait K8sService: Clone + Send + Sync + 'static {
    async fn new(config: &Config) -> Result<Self, K8sError> where Self: Sized;
    async fn deploy_pod(&self) -> Result<PodRef, K8sError>;
    async fn wait_for_pod_ready(&self, pod_ref: &PodRef) -> Result<Pod, K8sError>;
    async fn port_forward(&self, pod_ref: &PodRef, local_port: u16) -> Result<PortForwardHandle, K8sError>;
    async fn delete_pod(&self, pod_ref: &PodRef) -> Result<(), K8sError>;
}

#[derive(Clone)]
pub struct K8sServiceImpl {
    client: Client,
    config: Config,
}

fn generate_pod_name() -> String {
    let mut rng = rand::thread_rng();
    let random_hex: String = (0..6).map(|_| format!("{:x}", rng.gen_range(0..16))).collect();
    format!("k8socks-{}", random_hex)
}

fn build_pod_manifest(config: &Config, name: &str, ssh_key_base64: &str) -> Pod {
    let cfg = config;
    Pod {
        metadata: ObjectMeta {
            name: Some(name.to_string()),
            namespace: cfg.namespace.clone(),
            labels: cfg.pod_labels.clone().map(BTreeMap::from_iter),
            annotations: cfg.pod_annotations.clone().map(BTreeMap::from_iter),
            ..Default::default()
        },
        spec: Some(PodSpec {
            containers: vec![Container {
                name: "sshd".to_string(),
                image: cfg.pod_image.clone(),
                image_pull_policy: Some("IfNotPresent".to_string()),
                command: Some(vec![
                    "/bin/sh".to_string(),
                    "-c".to_string(),
                    format!(
                        "echo \"$SSH_PUBLIC_KEY\" | base64 -d > /tmp/authorized_keys && \
                         /usr/sbin/sshd -D -o 'AuthorizedKeysFile /tmp/authorized_keys' & \
                         PID=$! && sleep {} && kill $PID",
                        cfg.pod_ttl_seconds.unwrap_or(900)
                    ),
                ]),
                env: Some(vec![k8s_openapi::api::core::v1::EnvVar {
                    name: "SSH_PUBLIC_KEY".to_string(),
                    value: Some(ssh_key_base64.to_string()),
                    ..Default::default()
                }]),
                resources: cfg.pod_resources.as_ref().map(|r| ResourceRequirements {
                    requests: Some(
                        [
                            ("cpu".to_string(), Quantity(r.cpu.clone().unwrap())),
                            ("memory".to_string(), Quantity(r.memory.clone().unwrap())),
                        ]
                        .into_iter()
                        .collect(),
                    ),
                    ..Default::default()
                }),
                ..Default::default()
            }],
            ..Default::default()
        }),
        ..Default::default()
    }
}

#[async_trait]
impl K8sService for K8sServiceImpl {
    async fn new(config: &Config) -> Result<Self, K8sError> {
        let kubeconfig = KubeConfig::infer().await?;
        let client = Client::try_from(kubeconfig)?;
        Ok(Self {
            client,
            config: config.clone(),
        })
    }

    async fn deploy_pod(&self) -> Result<PodRef, K8sError> {
        let pod_name = generate_pod_name();
        let namespace = self.config.namespace.as_ref().unwrap();
        let pods: Api<Pod> = Api::namespaced(self.client.clone(), namespace);

        let ssh_key_path_str = self.config.ssh_public_key_path.as_ref().unwrap();
        let ssh_key_path = k8socks_config::expand_tilde(ssh_key_path_str).unwrap();
        let ssh_key_content = fs::read_to_string(&ssh_key_path)
            .map_err(|e| K8sError::SshKeyError(ssh_key_path.to_string_lossy().into(), e))?;
        let ssh_key_base64 = BASE64.encode(ssh_key_content.trim());

        let pod_manifest = build_pod_manifest(&self.config, &pod_name, &ssh_key_base64);
        pods.create(&PostParams::default(), &pod_manifest).await?;

        Ok(PodRef {
            name: pod_name,
            namespace: namespace.clone(),
        })
    }

    async fn wait_for_pod_ready(&self, pod_ref: &PodRef) -> Result<Pod, K8sError> {
        let api: Api<Pod> = Api::namespaced(self.client.clone(), &pod_ref.namespace);
        let establish = await_condition(api.clone(), &pod_ref.name, conditions::is_pod_running());
        let _ = tokio::time::timeout(Duration::from_secs(60), establish)
            .await
            .map_err(|_| K8sError::PodNotReady)?;
        api.get(&pod_ref.name).await.map_err(K8sError::Kube)
    }

    async fn port_forward(&self, pod_ref: &PodRef, local_port: u16) -> Result<PortForwardHandle, K8sError> {
        let pods: Api<Pod> = Api::namespaced(self.client.clone(), &pod_ref.namespace);
        let mut pf = pods.portforward(&pod_ref.name, &[22]).await?;

        let handle = tokio::spawn(async move {
            let mut upstream = pf.take_stream(22).expect("port 22 should be forwarded");
            let listener = tokio::net::TcpListener::bind(("127.0.0.1", local_port))
                .await
                .unwrap();

            if let Ok((mut downstream, _)) = listener.accept().await {
                io::copy_bidirectional(&mut upstream, &mut downstream).await.ok();
            }
        });

        Ok(PortForwardHandle {
            local_port,
            _handle: handle,
        })
    }

    async fn delete_pod(&self, pod_ref: &PodRef) -> Result<(), K8sError> {
        let api: Api<Pod> = Api::namespaced(self.client.clone(), &pod_ref.namespace);
        api.delete(&pod_ref.name, &DeleteParams::default()).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use k8socks_config::Config;
    use regex::Regex;

    #[test]
    fn test_generate_pod_name() {
        let name = generate_pod_name();
        let re = Regex::new(r"^k8socks-[0-9a-f]{6}$").unwrap();
        assert!(re.is_match(&name));
    }

    #[test]
    fn test_build_pod_manifest() {
        let config = Config {
            pod_image: Some("test-image:1.2.3".to_string()),
            pod_ttl_seconds: Some(3600),
            ..Default::default()
        };

        let pod_name = "k8socks-test123";
        let ssh_key = "ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABAQD...";
        let pod = build_pod_manifest(&config, pod_name, ssh_key);

        assert_eq!(pod.metadata.name.unwrap(), pod_name);
        let container = &pod.spec.as_ref().unwrap().containers[0];
        assert_eq!(container.image.as_ref().unwrap(), "test-image:1.2.3");

        // Check command for TTL
        let command_str = &container.command.as_ref().unwrap()[2];
        assert!(command_str.contains("sleep 3600"));

        // Check env var for SSH key
        let env_var = &container.env.as_ref().unwrap()[0];
        assert_eq!(env_var.name, "SSH_PUBLIC_KEY");
        assert_eq!(env_var.value.as_ref().unwrap(), ssh_key);
    }
}