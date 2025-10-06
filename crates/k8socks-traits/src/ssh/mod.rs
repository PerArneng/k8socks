use async_trait::async_trait;
use thiserror::Error;
use tokio::process::Child;
use crate::config::Config;

#[derive(Error, Debug)]
pub enum SshError {
    #[error("Failed to start SSH process: {0}")]
    ProcessError(#[from] std::io::Error),
    #[error("SSH process exited with a non-zero status")]
    UnexpectedExit,
}

/// A handle to a running SSH client subprocess.
pub struct SshProcessHandle {
    pub child: Child,
}

/// The `SshService` trait defines the contract for managing the local SSH SOCKS proxy.
#[async_trait]
pub trait SshService {
    fn new(config: &Config) -> Self;
    async fn start_socks_proxy(&self, forwarded_ssh_port: u16) -> Result<SshProcessHandle, SshError>;
    async fn watch(&self, handle: SshProcessHandle) -> Result<(), SshError>;
}