use async_trait::async_trait;
use kube::config::{InferConfigError, KubeconfigError};
use kube::Error as KubeError;
use thiserror::Error;
use tokio::task::JoinHandle;
use k8s_openapi::api::core::v1::Pod;
use crate::config::Config;

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

impl PortForwardHandle {
    pub fn new(local_port: u16, handle: JoinHandle<()>) -> Self {
        Self {
            local_port,
            _handle: handle,
        }
    }
}

#[async_trait]
pub trait K8sService: Clone + Send + Sync + 'static {
    async fn new(config: &Config) -> Result<Self, K8sError> where Self: Sized;
    async fn deploy_pod(&self) -> Result<PodRef, K8sError>;
    async fn wait_for_pod_ready(&self, pod_ref: &PodRef) -> Result<Pod, K8sError>;
    async fn port_forward(&self, pod_ref: &PodRef, local_port: u16) -> Result<PortForwardHandle, K8sError>;
    async fn delete_pod(&self, pod_ref: &PodRef) -> Result<(), K8sError>;
}