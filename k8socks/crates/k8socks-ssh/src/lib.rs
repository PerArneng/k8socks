use async_trait::async_trait;
use thiserror::Error;
use tokio::process::{Child, Command};
use tracing::{info, error};

use k8socks_config::Config;

#[derive(Error, Debug)]
pub enum SshError {
    #[error("Failed to spawn SSH process: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("SSH process exited with a non-zero status")]
    UnexpectedExit,
}

/// A handle to a running SSH client subprocess.
pub struct SshProcessHandle {
    child: Child,
}

impl Drop for SshProcessHandle {
    fn drop(&mut self) {
        info!("Terminating SSH SOCKS proxy process...");
        if let Err(e) = self.child.start_kill() {
            error!("Failed to kill SSH subprocess: {}", e);
        }
    }
}

/// The `SshService` trait defines the contract for managing the local SSH SOCKS proxy.
#[async_trait]
pub trait SshService {
    fn new(config: &Config) -> Self;
    async fn start_socks_proxy(&self, forwarded_ssh_port: u16) -> Result<SshProcessHandle, SshError>;
    async fn watch(&self, handle: SshProcessHandle) -> Result<(), SshError>;
}

pub struct SshServiceImpl {
    config: Config,
}

#[async_trait]
impl SshService for SshServiceImpl {
    fn new(config: &Config) -> Self {
        Self {
            config: config.clone(),
        }
    }

    async fn start_socks_proxy(
        &self,
        forwarded_ssh_port: u16,
    ) -> Result<SshProcessHandle, SshError> {
        let local_socks_port = self.config.local_socks_port.unwrap_or(1080);
        let ssh_username = self.config.ssh_username.as_ref().unwrap();

        let mut cmd = Command::new("ssh");
        cmd.arg("-o")
            .arg("StrictHostKeyChecking=no")
            .arg("-N")
            .arg("-D")
            .arg(local_socks_port.to_string())
            .arg("-p")
            .arg(forwarded_ssh_port.to_string())
            .arg(format!("{}@127.0.0.1", ssh_username));

        info!("Spawning SSH command: {:?}", cmd);

        let child = cmd.spawn()?;

        Ok(SshProcessHandle { child })
    }

    async fn watch(&self, mut handle: SshProcessHandle) -> Result<(), SshError> {
        let status = handle.child.wait().await?;
        if status.success() {
            info!("SSH process exited gracefully.");
            Ok(())
        } else {
            error!("SSH process exited with status: {}", status);
            Err(SshError::UnexpectedExit)
        }
    }
}