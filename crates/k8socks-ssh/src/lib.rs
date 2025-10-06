use std::process::Stdio;
use async_trait::async_trait;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, ChildStderr, ChildStdout, Command};
use tracing::{error, info, warn};

use k8socks_config::Config;

#[derive(Error, Debug)]
pub enum SshError {
    #[error("Failed to start SSH process: {0}")]
    ProcessError(#[from] std::io::Error),
    #[error("SSH process exited with a non-zero status")]
    UnexpectedExit,
}

/// A handle to a running SSH client subprocess.
pub struct SshProcessHandle {
    child: Child,
    stdout: Option<ChildStdout>,
    stderr: Option<ChildStderr>,
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
            .arg("-v") // Add verbosity to get connection logs
            .arg("-N") // Do not execute a remote command
            .arg("-D")
            .arg(local_socks_port.to_string())
            .arg("-p")
            .arg(forwarded_ssh_port.to_string())
            .arg(format!("{}@127.0.0.1", ssh_username));

        // Pipe stdout and stderr to capture them
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        info!("Spawning SSH command: {:?}", cmd);

        let mut child = cmd.spawn()?;
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        Ok(SshProcessHandle { child, stdout, stderr })
    }

    async fn watch(&self, mut handle: SshProcessHandle) -> Result<(), SshError> {
        let stdout = handle.stdout.take().ok_or_else(|| {
            SshError::ProcessError(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Failed to capture stdout",
            ))
        })?;

        let stderr = handle.stderr.take().ok_or_else(|| {
            SshError::ProcessError(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Failed to capture stderr",
            ))
        })?;

        let mut stdout_reader = BufReader::new(stdout).lines();
        let mut stderr_reader = BufReader::new(stderr).lines();

        let stdout_task = tokio::spawn(async move {
            while let Ok(Some(line)) = stdout_reader.next_line().await {
                info!("[ssh] {}", line);
            }
        });

        let stderr_task = tokio::spawn(async move {
            while let Ok(Some(line)) = stderr_reader.next_line().await {
                warn!("[ssh] {}", line);
            }
        });

        let status = handle.child.wait().await?;

        // Wait for the logging tasks to finish to ensure all output is captured.
        stdout_task.await.ok();
        stderr_task.await.ok();

        if status.success() {
            info!("SSH process exited gracefully.");
            Ok(())
        } else {
            error!("SSH process exited with status: {}", status);
            Err(SshError::UnexpectedExit)
        }
    }
}