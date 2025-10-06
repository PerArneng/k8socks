# k8socks

`k8socks` is a command-line tool that provides a temporary, secure SOCKS5 proxy into a Kubernetes cluster. It works by deploying a short-lived SSH server pod and forwarding a local port to it, giving you instant access to your cluster's network.

## High-Level Architecture

The tool is built as a native Rust application with a clean, modular architecture. The core logic is separated into several crates, each with a distinct responsibility:

-   `k8socks-cli`: The main application entrypoint, responsible for parsing CLI arguments and orchestrating the other components.
-   `k8socks-config`: Handles loading and merging configuration from files and CLI flags.
-   `k8socks-logging`: Provides a custom-formatted, colorized logger.
-   `k8socks-k8s`: Contains all the logic for interacting with the Kubernetes API via `kube-rs`. It manages the lifecycle of the SSH pod.
-   `k8socks-ssh`: Manages the local `ssh` client subprocess that creates the SOCKS5 proxy.

The contracts between these services are defined using standard Rust traits, ensuring clear boundaries and testability.

## Requirements

-   A running Kubernetes cluster.
-   `kubectl` configured to connect to your cluster.
-   A local `ssh` client available in your system's `PATH`.
-   An SSH public key (e.g., `~/.ssh/id_rsa.pub`).

## Installation

1.  Download the latest release binary for your platform (e.g., `k8socks-x86_64-unknown-linux-gnu`) from the [GitHub Releases](https://github.com/your-repo/k8socks/releases) page.
2.  Make the binary executable: `chmod +x ./k8socks-...`
3.  Place it in a directory in your `PATH`, for example: `mv ./k8socks-... /usr/local/bin/k8socks`

## Quickstart

1.  **Create a configuration file.** By default, `k8socks` looks for a configuration file at `~/.k8socks/config.json`. Create this file with your details:

    ```json
    {
      "kubeconfig": "~/.kube/config",
      "context": "your-context-name",
      "namespace": "default",
      "ssh_public_key_path": "~/.ssh/id_rsa.pub",
      "local_socks_port": 1080
    }
    ```

2.  **Run the deploy command.**

    ```bash
    k8socks deploy
    ```

    You can override any configuration setting with CLI flags:

    ```bash
    k8socks --namespace my-namespace --local-socks-port 9999 deploy
    ```

3.  **Configure your browser** or application to use the SOCKS5 proxy at `127.0.0.1:1080` (or the port you specified).

4.  Press `Ctrl+C` in the terminal to shut down the proxy. This will automatically delete the SSH pod from your cluster.

## Configuration & Flags

Configuration is loaded in the following order of precedence, with later sources overriding earlier ones:

1.  **Built-in Defaults**
2.  **Configuration File** (`~/.k8socks/config.json` or `./config.json`)
3.  **CLI Flags**

### All Configuration Options

| JSON Key              | CLI Flag                  | Default                               | Description                                                 |
| --------------------- | ------------------------- | ------------------------------------- | ----------------------------------------------------------- |
| `kubeconfig`          | `--kubeconfig`            | `~/.kube/config`                      | Path to your kubeconfig file.                               |
| `context`             | `--context`               | (none)                                | The Kubernetes context to use.                              |
| `namespace`           | `--namespace`             | `default`                             | The namespace to deploy the pod in.                         |
| `ssh_public_key_path` | `--ssh-public-key-path`   | `~/.ssh/id_rsa.pub`                   | Path to your SSH public key.                                |
| `ssh_username`        | `--ssh-username`          | `k8socks`                             | The username for the SSH connection.                        |
| `local_socks_port`    | `--local-socks-port`      | `1080`                                | The local port for the SOCKS5 proxy.                        |
| `pod_ttl_seconds`     | `--pod-ttl-seconds`       | `900`                                 | Time in seconds before the pod self-destructs.              |
| `pod_image`           | `--pod-image`             | `linuxserver/openssh-server:latest`   | The container image for the SSH server pod.                 |
| `log_level`           | `--log-level`             | `info`                                | Log level (`trace`, `debug`, `info`, `warn`, `error`).      |

### CLI-Only Flags

-   `--config <path>`: Path to a custom configuration file.
-   `--no-color`: Disable colored output in logs.
-   `--non-interactive`: Fail instead of prompting for user input (currently no interactive prompts exist).
-   `--dry-run`: Print the generated Kubernetes manifest and intended actions without executing them.

## Security Notes

-   **Ephemeral Pod:** The SSH server pod is designed to be short-lived. It automatically self-destructs after the configured TTL (`pod_ttl_seconds`) to minimize its footprint.
-   **Graceful Cleanup:** The tool is designed to delete the pod immediately upon exit (`Ctrl+C`), ensuring no resources are left behind.
-   **SSH Key:** Your public SSH key is injected into the pod to authorize your connection. Your private key never leaves your local machine.

## Development Guide

This project is a Rust workspace.

-   **Workspace Layout:** The code is organized into several crates under the `/crates` directory.
-   **Building:** `cargo build --workspace`
-   **Testing:** `cargo test --workspace`
-   **Running:** `cargo run -p k8socks-cli -- [FLAGS] deploy`

The core logic is abstracted behind the `K8sService` and `SshService` traits, making it easy to test and reason about different components in isolation.