use std::process::Stdio;
use async_trait::async_trait;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Command};
use tracing::{error, info, warn};

use k8socks_traits::config::Config;
use k8socks_traits::ssh::{SshError, SshProcessHandle, SshService};

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

        let child = cmd.spawn()?;

        Ok(SshProcessHandle { child })
    }

    async fn watch(&self, handle: SshProcessHandle) -> Result<(), SshError> {
        let mut child = handle.child;
        let stdout = child.stdout.take().ok_or_else(|| {
            SshError::ProcessError(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Failed to capture stdout",
            ))
        })?;

        let stderr = child.stderr.take().ok_or_else(|| {
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

        let status = child.wait().await?;

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