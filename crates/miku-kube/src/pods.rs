use async_trait::async_trait;
use futures::channel::mpsc;
use futures::{AsyncBufReadExt, SinkExt, StreamExt, TryStreamExt};
use kube::Api;
use kube::api::{AttachParams, AttachedProcess, LogParams, TerminalSize};
use miku_api::{
    ClusterConfigStore, ClusterRegistry, LocalPreferenceStore, LogLine, PodAttachInput,
    PodAttachOutput, PodAttachRequest, PodAttachService, PodAttachSession, PodExecRequest,
    PodExecService, PodLogQuery, PodLogService, default_pod_exec_command,
};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::client::KubeServices;

#[async_trait]
impl<S> PodLogService for KubeServices<S>
where
    S: ClusterConfigStore + ClusterRegistry + LocalPreferenceStore + Clone + Send + Sync,
{
    #[tracing::instrument(name = "kube.read_logs", skip(self), fields(namespace = %query.namespace, pod = %query.pod))]
    async fn read_logs(&self, query: PodLogQuery) -> miku_core::Result<Vec<LogLine>> {
        let client = self.client_for_cluster(&query.cluster_id).await?;
        let pods: Api<k8s_openapi::api::core::v1::Pod> = Api::namespaced(client, &query.namespace);
        let params = log_params(&query);
        let logs = pods
            .logs(&query.pod, &params)
            .await
            .map_err(|error| miku_core::MikuError::Kubernetes(error.to_string()))?;

        Ok(logs
            .lines()
            .map(|line| LogLine {
                text: line.to_owned(),
            })
            .collect())
    }

    #[tracing::instrument(name = "kube.stream_logs", skip(self), fields(namespace = %query.namespace, pod = %query.pod))]
    async fn stream_logs(
        &self,
        query: PodLogQuery,
    ) -> miku_core::Result<miku_api::BoxEventStream<LogLine>> {
        let client = self.client_for_cluster(&query.cluster_id).await?;
        let pods: Api<k8s_openapi::api::core::v1::Pod> = Api::namespaced(client, &query.namespace);
        let mut params = log_params(&query);
        params.follow = true;
        let lines = pods
            .log_stream(&query.pod, &params)
            .await
            .map_err(|error| miku_core::MikuError::Kubernetes(error.to_string()))?
            .lines();

        Ok(futures::stream::try_unfold(lines, |mut lines| async move {
            let line = lines
                .try_next()
                .await
                .map_err(|error| miku_core::MikuError::Kubernetes(error.to_string()))?;
            Ok(line.map(|text| (LogLine { text }, lines)))
        })
        .boxed())
    }
}

#[async_trait]
impl<S> PodAttachService for KubeServices<S>
where
    S: ClusterConfigStore + ClusterRegistry + LocalPreferenceStore + Clone + Send + Sync,
{
    #[tracing::instrument(name = "kube.attach_pod", skip(self), fields(namespace = %request.namespace, pod = %request.pod))]
    async fn attach_pod(&self, request: PodAttachRequest) -> miku_core::Result<PodAttachSession> {
        let client = self.client_for_cluster(&request.cluster_id).await?;
        let pods: Api<k8s_openapi::api::core::v1::Pod> =
            Api::namespaced(client, &request.namespace);
        let params = attach_params(&request);
        let attached = pods
            .attach(&request.pod, &params)
            .await
            .map_err(|error| miku_core::MikuError::Kubernetes(error.to_string()))?;

        Ok(attached_process_session(attached))
    }
}

#[async_trait]
impl<S> PodExecService for KubeServices<S>
where
    S: ClusterConfigStore + ClusterRegistry + LocalPreferenceStore + Clone + Send + Sync,
{
    #[tracing::instrument(name = "kube.exec_pod", skip(self), fields(namespace = %request.namespace, pod = %request.pod))]
    async fn exec_pod(&self, request: PodExecRequest) -> miku_core::Result<PodAttachSession> {
        let client = self.client_for_cluster(&request.cluster_id).await?;
        let pods: Api<k8s_openapi::api::core::v1::Pod> =
            Api::namespaced(client, &request.namespace);
        let params = exec_params(&request);
        let command = exec_command(&request);
        let attached = pods
            .exec(&request.pod, command, &params)
            .await
            .map_err(|error| miku_core::MikuError::Kubernetes(error.to_string()))?;

        Ok(attached_process_session(attached))
    }
}

enum PodAttachOutputKind {
    Stdout,
    Stderr,
}

fn attached_process_session(mut attached: AttachedProcess) -> PodAttachSession {
    let stdin = attached.stdin();
    let stdout = attached.stdout();
    let stderr = attached.stderr();
    let terminal_size = attached.terminal_size();
    let (input_tx, input_rx) = mpsc::unbounded();
    let (output_tx, output_rx) = mpsc::unbounded();

    tokio::spawn(run_attach_input(input_rx, stdin, terminal_size));
    if let Some(stdout) = stdout {
        tokio::spawn(read_attach_output(
            stdout,
            output_tx.clone(),
            PodAttachOutputKind::Stdout,
        ));
    }
    if let Some(stderr) = stderr {
        tokio::spawn(read_attach_output(
            stderr,
            output_tx.clone(),
            PodAttachOutputKind::Stderr,
        ));
    }
    tokio::spawn(async move {
        let result = attached
            .join()
            .await
            .map_err(|error| miku_core::MikuError::Kubernetes(error.to_string()));
        if let Err(error) = result {
            let _ = output_tx.unbounded_send(Err(error));
        }
        let _ = output_tx.unbounded_send(Ok(PodAttachOutput::Closed));
    });

    PodAttachSession {
        input: input_tx,
        output: output_rx.boxed(),
    }
}

async fn run_attach_input<W>(
    mut input: mpsc::UnboundedReceiver<PodAttachInput>,
    mut stdin: Option<W>,
    mut terminal_size: Option<impl futures::Sink<TerminalSize> + Unpin>,
) where
    W: AsyncWrite + Unpin,
{
    while let Some(message) = input.next().await {
        match message {
            PodAttachInput::Bytes(bytes) => {
                if let Some(stdin) = stdin.as_mut()
                    && stdin.write_all(&bytes).await.is_err()
                {
                    break;
                }
            }
            PodAttachInput::Resize { cols, rows } => {
                if let Some(terminal_size) = terminal_size.as_mut() {
                    let _ = terminal_size
                        .send(TerminalSize {
                            width: cols,
                            height: rows,
                        })
                        .await;
                }
            }
            PodAttachInput::Close => break,
        }
    }
}

async fn read_attach_output<R>(
    mut reader: R,
    output: mpsc::UnboundedSender<miku_core::Result<PodAttachOutput>>,
    kind: PodAttachOutputKind,
) where
    R: AsyncRead + Unpin,
{
    let mut buffer = [0_u8; 4096];
    loop {
        match reader.read(&mut buffer).await {
            Ok(0) => break,
            Ok(count) => {
                let bytes = buffer[..count].to_vec();
                let message = match kind {
                    PodAttachOutputKind::Stdout => PodAttachOutput::Stdout(bytes),
                    PodAttachOutputKind::Stderr => PodAttachOutput::Stderr(bytes),
                };
                if output.unbounded_send(Ok(message)).is_err() {
                    break;
                }
            }
            Err(error) => {
                let _ =
                    output.unbounded_send(Err(miku_core::MikuError::Kubernetes(error.to_string())));
                break;
            }
        }
    }
}

fn attach_params(request: &PodAttachRequest) -> AttachParams {
    interactive_params(request.container.as_deref(), request.tty)
}

fn exec_params(request: &PodExecRequest) -> AttachParams {
    interactive_params(request.container.as_deref(), request.tty)
}

fn interactive_params(container: Option<&str>, tty: bool) -> AttachParams {
    let mut params = AttachParams::interactive_tty().tty(tty).stderr(!tty);
    if let Some(container) = container {
        params = params.container(container);
    }
    params
}

fn exec_command(request: &PodExecRequest) -> Vec<String> {
    if request.command.is_empty() {
        default_pod_exec_command()
    } else {
        request.command.clone()
    }
}

fn log_params(query: &PodLogQuery) -> LogParams {
    LogParams {
        container: query.container.clone(),
        tail_lines: query.tail_lines.map(i64::from),
        ..LogParams::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_params_map_container_and_tail_lines() {
        let query = miku_api::PodLogQuery {
            cluster_id: miku_core::ClusterId::new("local"),
            namespace: "default".to_owned(),
            pod: "api".to_owned(),
            container: Some("server".to_owned()),
            tail_lines: Some(100),
        };

        let params = log_params(&query);

        assert_eq!(params.container.as_deref(), Some("server"));
        assert_eq!(params.tail_lines, Some(100));
    }

    #[test]
    fn attach_params_use_tty_without_stderr_and_container() {
        let request = miku_api::PodAttachRequest {
            cluster_id: miku_core::ClusterId::new("local"),
            namespace: "default".to_owned(),
            pod: "api".to_owned(),
            container: Some("server".to_owned()),
            tty: true,
        };

        let params = attach_params(&request);

        assert!(params.stdin);
        assert!(params.stdout);
        assert!(params.tty);
        assert!(!params.stderr);
        assert_eq!(params.container.as_deref(), Some("server"));
    }

    #[test]
    fn attach_params_enable_stderr_when_tty_is_disabled() {
        let request = miku_api::PodAttachRequest {
            cluster_id: miku_core::ClusterId::new("local"),
            namespace: "default".to_owned(),
            pod: "api".to_owned(),
            container: None,
            tty: false,
        };

        let params = attach_params(&request);

        assert!(params.stdin);
        assert!(params.stdout);
        assert!(!params.tty);
        assert!(params.stderr);
        assert!(params.container.is_none());
    }

    #[test]
    fn exec_params_use_tty_without_stderr_and_container() {
        let request = miku_api::PodExecRequest {
            cluster_id: miku_core::ClusterId::new("local"),
            namespace: "default".to_owned(),
            pod: "api".to_owned(),
            container: Some("server".to_owned()),
            command: vec!["/bin/sh".to_owned()],
            tty: true,
        };

        let params = exec_params(&request);

        assert!(params.stdin);
        assert!(params.stdout);
        assert!(params.tty);
        assert!(!params.stderr);
        assert_eq!(params.container.as_deref(), Some("server"));
    }

    #[test]
    fn exec_command_defaults_to_shell_when_empty() {
        let request = miku_api::PodExecRequest {
            cluster_id: miku_core::ClusterId::new("local"),
            namespace: "default".to_owned(),
            pod: "api".to_owned(),
            container: None,
            command: Vec::new(),
            tty: true,
        };

        assert_eq!(exec_command(&request), miku_api::default_pod_exec_command());
    }
}
