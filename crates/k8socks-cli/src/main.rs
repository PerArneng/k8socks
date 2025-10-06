use clap::{Parser, Subcommand};
use k8socks_config::{Config, load_from_paths, expand_tilde};
use k8socks_k8s::{K8sService, K8sServiceImpl, PodRef};
use k8socks_logging::init_logging;
use k8socks_ssh::{SshService, SshServiceImpl};
use merge::Merge;
use tokio::signal;
use tracing::{info, warn, debug, error};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    #[arg(long)]
    pub kubeconfig: Option<String>,
    #[arg(long)]
    pub context: Option<String>,
    #[arg(long)]
    pub namespace: Option<String>,
    #[arg(long)]
    pub ssh_public_key_path: Option<String>,
    #[arg(long)]
    pub ssh_username: Option<String>,
    #[arg(long)]
    pub local_socks_port: Option<u16>,
    #[arg(long)]
    pub pod_ttl_seconds: Option<u64>,
    #[arg(long)]
    pub pod_image: Option<String>,
    #[arg(long)]
    pub log_level: Option<String>,
    #[arg(long)]
    pub config: Option<String>,
    #[arg(long)]
    pub no_color: bool,
    #[arg(long)]
    pub non_interactive: bool,
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Deploys the SSH pod and starts the SOCKS5 proxy.
    Deploy,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // --- Configuration Setup ---
    let file_config = load_from_paths()?;
    let cli_config = Config {
        kubeconfig: cli.kubeconfig,
        context: cli.context,
        namespace: cli.namespace,
        ssh_public_key_path: cli.ssh_public_key_path,
        ssh_username: cli.ssh_username,
        local_socks_port: cli.local_socks_port,
        pod_ttl_seconds: cli.pod_ttl_seconds,
        pod_image: cli.pod_image,
        pod_resources: None,
        pod_labels: None,
        pod_annotations: None,
        log_level: cli.log_level,
    };
    let mut config = Config::default();
    config.merge(file_config);
    config.merge(cli_config);

    // --- Logging ---
    init_logging(config.log_level.as_deref().unwrap_or("info"), !cli.no_color)
        .map_err(|e| anyhow::anyhow!("Failed to initialize logging: {}", e))?;

    // --- Path Expansion ---
    if let Some(path) = config.kubeconfig.clone() {
        config.kubeconfig = Some(expand_tilde(&path).unwrap().to_string_lossy().into_owned());
    }
    if let Some(path) = config.ssh_public_key_path.clone() {
        config.ssh_public_key_path = Some(expand_tilde(&path).unwrap().to_string_lossy().into_owned());
    }

    debug!("Final configuration: {:#?}", config);

    if cli.dry_run {
        info!("[dry-run] Would execute the following steps:");
        info!("[dry-run] 1. Connect to Kubernetes cluster");
        info!("[dry-run] 2. Deploy a pod with image '{}'", config.pod_image.as_ref().unwrap());
        info!("[dry-run] 3. Wait for pod to become ready");
        info!("[dry-run] 4. Establish port-forward to pod:22");
        info!("[dry-run] 5. Start local SSH SOCKS5 proxy on port {}", config.local_socks_port.unwrap_or(1080));
        info!("[dry-run] 6. On exit, delete the pod");
        return Ok(());
    }

    // --- Main Application Logic ---
    let k8s_service = K8sServiceImpl::new(&config).await?;
    let pod_ref = deploy_and_wait(&k8s_service).await?;

    // Set up graceful shutdown
    let (tx, mut rx) = tokio::sync::mpsc::channel(1);
    let k8s_service_clone = k8s_service.clone();
    let pod_ref_clone = pod_ref.clone();

    tokio::spawn(async move {
        signal::ctrl_c().await.expect("Failed to listen for ctrl-c");
        warn!("Received shutdown signal. Cleaning up...");
        if let Err(e) = k8s_service_clone.delete_pod(&pod_ref_clone).await {
            error!("Failed to delete pod during shutdown: {}", e);
        }
        tx.send(()).await.ok();
    });

    // Start port forwarding and the SSH proxy
    let local_socks_port = config.local_socks_port.unwrap_or(1080);
    let pf_handle = k8s_service.port_forward(&pod_ref, local_socks_port).await?;
    let ssh_service = SshServiceImpl::new(&config);
    let ssh_handle = ssh_service.start_socks_proxy(pf_handle.local_port).await?;
    info!("SOCKS5 proxy is now running on 127.0.0.1:{}", config.local_socks_port.unwrap_or(1080));
    info!("Press Ctrl+C to exit.");

    // Wait for either the SSH process to exit or for a shutdown signal
    tokio::select! {
        res = ssh_service.watch(ssh_handle) => {
            if let Err(e) = res {
                error!("SSH process failed: {}", e);
            }
        }
        _ = rx.recv() => {
            info!("Shutdown complete.");
        }
    }

    // Final cleanup in case of non-Ctrl+C exit
    if rx.try_recv().is_err() {
        info!("Cleaning up pod...");
        if let Err(e) = k8s_service.delete_pod(&pod_ref).await {
            error!("Failed to delete pod on exit: {}", e);
        }
    }

    Ok(())
}

async fn deploy_and_wait(k8s_service: &K8sServiceImpl) -> anyhow::Result<PodRef> {
    info!("Deploying SSH server pod...");
    let pod_ref = k8s_service.deploy_pod().await?;
    info!("Pod '{}' created in namespace '{}'. Waiting for it to be ready...", pod_ref.name, pod_ref.namespace);
    k8s_service.wait_for_pod_ready(&pod_ref).await?;
    info!("Pod is running and ready.");
    Ok(pod_ref)
}