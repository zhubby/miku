use std::sync::Arc;

use async_trait::async_trait;
use miku_api::{
    AgentEvent, AgentToolCallSummary, AgentTurnStatus, ClusterStatusRequest, PodLogQuery,
    ResourceQuery,
};
use miku_core::{ClusterId, ResourceRef};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::AgentToolServices;
use crate::provider::ToolDefinition;

#[derive(Clone, Debug)]
pub struct ToolOutput {
    pub content: String,
    pub completion: Option<CompletionSignal>,
}

#[derive(Clone, Debug)]
pub struct CompletionSignal {
    pub status: AgentTurnStatus,
    pub summary: String,
}

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn parameters(&self) -> Value;

    async fn execute(
        &self,
        args: Value,
        services: Arc<dyn AgentToolServices>,
    ) -> miku_core::Result<ToolOutput>;

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name().to_owned(),
            description: self.description().to_owned(),
            parameters: self.parameters(),
        }
    }
}

#[derive(Clone)]
pub struct ToolRegistry {
    tools: Vec<Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn miku_readonly() -> Self {
        Self {
            tools: vec![
                Arc::new(ListClustersTool),
                Arc::new(GetClusterStatusTool),
                Arc::new(ListResourcesTool),
                Arc::new(GetResourceTool),
                Arc::new(ReadPodLogsTool),
                Arc::new(CompleteTaskTool),
            ],
        }
    }

    pub fn definitions(&self) -> Vec<ToolDefinition> {
        self.tools.iter().map(|tool| tool.definition()).collect()
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.iter().find(|tool| tool.name() == name).cloned()
    }
}

pub struct ToolExecutionRecord {
    pub summary: AgentToolCallSummary,
    pub event: AgentEvent,
    pub output: Option<ToolOutput>,
}

impl ToolExecutionRecord {
    pub fn success(name: String, arguments: Value, result: String, output: ToolOutput) -> Self {
        Self {
            summary: AgentToolCallSummary {
                name: name.clone(),
                arguments,
                result: Some(result.clone()),
                error: None,
            },
            event: AgentEvent::ToolFinished { name, result },
            output: Some(output),
        }
    }

    pub fn failure(name: String, arguments: Value, error: String) -> Self {
        Self {
            summary: AgentToolCallSummary {
                name: name.clone(),
                arguments,
                result: None,
                error: Some(error.clone()),
            },
            event: AgentEvent::ToolFailed { name, error },
            output: None,
        }
    }
}

struct ListClustersTool;

#[async_trait]
impl Tool for ListClustersTool {
    fn name(&self) -> &'static str {
        "list_clusters"
    }

    fn description(&self) -> &'static str {
        "List configured Kubernetes clusters visible to Miku."
    }

    fn parameters(&self) -> Value {
        object_schema(vec![])
    }

    async fn execute(
        &self,
        _args: Value,
        services: Arc<dyn AgentToolServices>,
    ) -> miku_core::Result<ToolOutput> {
        let clusters = services.list_clusters().await?;
        json_output(&clusters)
    }
}

struct GetClusterStatusTool;

#[derive(Deserialize)]
struct ClusterStatusArgs {
    cluster_id: String,
}

#[async_trait]
impl Tool for GetClusterStatusTool {
    fn name(&self) -> &'static str {
        "get_cluster_status"
    }

    fn description(&self) -> &'static str {
        "Read a Kubernetes cluster status overview, conditions, workload counts, and recent events."
    }

    fn parameters(&self) -> Value {
        object_schema(vec![string_property("cluster_id", "Cluster identifier")])
    }

    async fn execute(
        &self,
        args: Value,
        services: Arc<dyn AgentToolServices>,
    ) -> miku_core::Result<ToolOutput> {
        let args: ClusterStatusArgs = parse_args(args)?;
        let status = services
            .get_cluster_status(ClusterStatusRequest {
                cluster_id: ClusterId::new(args.cluster_id),
            })
            .await?;
        json_output(&status)
    }
}

struct ListResourcesTool;

#[derive(Deserialize)]
struct ResourceArgs {
    cluster_id: String,
    group: Option<String>,
    version: String,
    plural: String,
    namespace: Option<String>,
    label_selector: Option<String>,
    limit: Option<u32>,
}

#[async_trait]
impl Tool for ListResourcesTool {
    fn name(&self) -> &'static str {
        "list_resources"
    }

    fn description(&self) -> &'static str {
        "List Kubernetes resources by group, version, plural, and optional namespace."
    }

    fn parameters(&self) -> Value {
        object_schema(vec![
            string_property("cluster_id", "Cluster identifier"),
            string_property("group", "Kubernetes API group; omit for core resources").optional(),
            string_property("version", "Kubernetes API version"),
            string_property(
                "plural",
                "Resource plural name, for example pods or deployments",
            ),
            string_property("namespace", "Namespace for namespaced resources").optional(),
            string_property("label_selector", "Optional Kubernetes label selector").optional(),
            integer_property("limit", "Maximum number of resources to return").optional(),
        ])
    }

    async fn execute(
        &self,
        args: Value,
        services: Arc<dyn AgentToolServices>,
    ) -> miku_core::Result<ToolOutput> {
        let args: ResourceArgs = parse_args(args)?;
        let query = resource_query_from_args(&args);
        let resources = services.list_resources(query).await?;
        json_output(&resources)
    }
}

struct GetResourceTool;

#[derive(Deserialize)]
struct ResourceDetailArgs {
    cluster_id: String,
    group: Option<String>,
    version: String,
    plural: String,
    namespace: Option<String>,
    name: String,
}

#[async_trait]
impl Tool for GetResourceTool {
    fn name(&self) -> &'static str {
        "get_resource"
    }

    fn description(&self) -> &'static str {
        "Read one Kubernetes resource by name."
    }

    fn parameters(&self) -> Value {
        object_schema(vec![
            string_property("cluster_id", "Cluster identifier"),
            string_property("group", "Kubernetes API group; omit for core resources").optional(),
            string_property("version", "Kubernetes API version"),
            string_property("plural", "Resource plural name"),
            string_property("namespace", "Namespace for namespaced resources").optional(),
            string_property("name", "Resource name"),
        ])
    }

    async fn execute(
        &self,
        args: Value,
        services: Arc<dyn AgentToolServices>,
    ) -> miku_core::Result<ToolOutput> {
        let args: ResourceDetailArgs = parse_args(args)?;
        let resource_args = ResourceArgs {
            cluster_id: args.cluster_id,
            group: args.group,
            version: args.version,
            plural: args.plural,
            namespace: args.namespace,
            label_selector: None,
            limit: Some(1),
        };
        let detail = services
            .get_resource(resource_query_from_args(&resource_args), &args.name)
            .await?;
        json_output(&detail)
    }
}

struct ReadPodLogsTool;

#[derive(Deserialize)]
struct PodLogsArgs {
    cluster_id: String,
    namespace: String,
    pod: String,
    container: Option<String>,
    tail_lines: Option<u32>,
}

#[async_trait]
impl Tool for ReadPodLogsTool {
    fn name(&self) -> &'static str {
        "read_pod_logs"
    }

    fn description(&self) -> &'static str {
        "Read recent logs from a Kubernetes pod."
    }

    fn parameters(&self) -> Value {
        object_schema(vec![
            string_property("cluster_id", "Cluster identifier"),
            string_property("namespace", "Pod namespace"),
            string_property("pod", "Pod name"),
            string_property("container", "Optional container name").optional(),
            integer_property("tail_lines", "Number of recent log lines to read").optional(),
        ])
    }

    async fn execute(
        &self,
        args: Value,
        services: Arc<dyn AgentToolServices>,
    ) -> miku_core::Result<ToolOutput> {
        let args: PodLogsArgs = parse_args(args)?;
        let logs = services
            .read_logs(PodLogQuery {
                cluster_id: ClusterId::new(args.cluster_id),
                namespace: args.namespace,
                pod: args.pod,
                container: args.container,
                tail_lines: args.tail_lines,
            })
            .await?;
        json_output(&logs)
    }
}

struct CompleteTaskTool;

#[derive(Deserialize)]
struct CompleteTaskArgs {
    summary: String,
    status: Option<String>,
}

#[async_trait]
impl Tool for CompleteTaskTool {
    fn name(&self) -> &'static str {
        "complete_task"
    }

    fn description(&self) -> &'static str {
        "Signal that the agent has completed, partially completed, or is blocked on the task."
    }

    fn parameters(&self) -> Value {
        object_schema(vec![
            string_property("summary", "Summary of what was accomplished"),
            string_property("status", "success, partial, or blocked").optional(),
        ])
    }

    async fn execute(
        &self,
        args: Value,
        _services: Arc<dyn AgentToolServices>,
    ) -> miku_core::Result<ToolOutput> {
        let args: CompleteTaskArgs = parse_args(args)?;
        let status = match args.status.as_deref() {
            Some("partial") => AgentTurnStatus::Partial,
            Some("blocked") => AgentTurnStatus::Blocked,
            _ => AgentTurnStatus::Completed,
        };
        Ok(ToolOutput {
            content: args.summary.clone(),
            completion: Some(CompletionSignal {
                status,
                summary: args.summary,
            }),
        })
    }
}

fn parse_args<T>(args: Value) -> miku_core::Result<T>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_value(args).map_err(|error| {
        miku_core::MikuError::Config(format!("invalid agent tool arguments: {error}"))
    })
}

fn json_output<T>(value: &T) -> miku_core::Result<ToolOutput>
where
    T: serde::Serialize,
{
    let content = serde_json::to_string_pretty(value)
        .map_err(|error| miku_core::MikuError::Transport(error.to_string()))?;
    Ok(ToolOutput {
        content,
        completion: None,
    })
}

fn resource_query_from_args(args: &ResourceArgs) -> ResourceQuery {
    let resource = match &args.group {
        Some(group) if !group.trim().is_empty() => {
            ResourceRef::grouped(group.clone(), args.version.clone(), args.plural.clone())
        }
        _ => ResourceRef::core(args.version.clone(), args.plural.clone()),
    };
    let resource = match &args.namespace {
        Some(namespace) if !namespace.trim().is_empty() => resource.namespaced(namespace.clone()),
        _ => resource,
    };

    ResourceQuery {
        cluster_id: ClusterId::new(args.cluster_id.clone()),
        resource,
        namespace: args.namespace.clone(),
        label_selector: args.label_selector.clone(),
        limit: args.limit.or(Some(250)),
    }
}

#[derive(Clone)]
struct SchemaProperty {
    name: &'static str,
    schema: Value,
    required: bool,
}

impl SchemaProperty {
    fn optional(mut self) -> Self {
        self.required = false;
        self
    }
}

fn string_property(name: &'static str, description: &'static str) -> SchemaProperty {
    SchemaProperty {
        name,
        schema: json!({"type": "string", "description": description}),
        required: true,
    }
}

fn integer_property(name: &'static str, description: &'static str) -> SchemaProperty {
    SchemaProperty {
        name,
        schema: json!({"type": "integer", "description": description}),
        required: true,
    }
}

fn object_schema(properties: Vec<SchemaProperty>) -> Value {
    let required = properties
        .iter()
        .filter(|property| property.required)
        .map(|property| property.name)
        .collect::<Vec<_>>();
    let properties = properties
        .into_iter()
        .map(|property| (property.name.to_owned(), property.schema))
        .collect::<serde_json::Map<_, _>>();

    json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false
    })
}
