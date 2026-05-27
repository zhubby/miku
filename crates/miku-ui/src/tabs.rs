use std::collections::HashMap;

use eframe::egui;
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use egui_dock::TabViewer;
use miku_api::{
    AgentContext, AgentConversationSummary, AgentEvent, AgentMessage, AgentRole, AgentTurnRequest,
    AgentTurnResponse, ClusterStatusReport, ClusterStatusSeverity, ClusterStatusWorkloadSummary,
    ClusterSummary,
};

use crate::resource_panel::{
    ClusterRoleBindingResourcePanel, ClusterRoleResourcePanel, ConfigMapResourcePanel,
    CronJobResourcePanel, CustomResourcesPanel, DaemonSetResourcePanel, DeploymentResourcePanel,
    EndpointSliceResourcePanel, EndpointsResourcePanel, EventResourcePanel,
    HorizontalPodAutoscalerResourcePanel, IngressClassResourcePanel, IngressResourcePanel,
    JobResourcePanel, LeaseResourcePanel, LimitRangeResourcePanel,
    MutatingWebhookConfigurationResourcePanel, NamespaceResourcePanel, NetworkPolicyResourcePanel,
    NodeResourcePanel, PersistentVolumeClaimResourcePanel, PersistentVolumeResourcePanel,
    PodAttachInputRequest, PodAttachRequest, PodDisruptionBudgetResourcePanel, PodLogRequest,
    PodResourcePanel, PriorityClassResourcePanel, ReplicaSetResourcePanel, ResourceActionRequest,
    ResourceLoadRequest, ResourcePanelRequests, ResourceQuotaResourcePanel, ResourceWatchRequest,
    RoleBindingResourcePanel, RoleResourcePanel, RuntimeClassResourcePanel, SecretResourcePanel,
    ServiceAccountResourcePanel, ServiceResourcePanel, StatefulSetResourcePanel,
    StorageClassResourcePanel, ValidatingWebhookConfigurationResourcePanel,
};
use crate::resources::{RESOURCE_CATEGORIES, ResourceNavItem};
use crate::state::{AppState, ClusterConnectionState};

const AGENT_COMPOSER_OUTER_HEIGHT: f32 = 148.0;
const AGENT_MESSAGE_FRAME_VERTICAL_INSET: f32 = 16.0;
const AGENT_SECTION_SPACING: f32 = 8.0;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum AppTab {
    Clusters,
    Resources,
    Workspace(usize),
    Resource(ResourceNavItem),
    Agent(usize),
}

pub(crate) struct AppTabViewer<'a> {
    pub(crate) state: &'a AppState,
    pub(crate) clusters: &'a [ClusterSummary],
    pub(crate) cluster_connection_states: &'a HashMap<miku_core::ClusterId, ClusterConnectionState>,
    pub(crate) cluster_load_in_flight: bool,
    pub(crate) cluster_load_error: Option<&'a str>,
    pub(crate) closeable: bool,
    pub(crate) allow_windows: bool,
    pub(crate) add_tab: Option<AppTab>,
    pub(crate) add_requested: bool,
    pub(crate) new_cluster_requested: bool,
    pub(crate) selected_cluster: Option<ClusterSummary>,
    pub(crate) active_resource: Option<ResourceNavItem>,
    pub(crate) selected_resource: Option<ResourceNavItem>,
    pub(crate) selected_cluster_id: Option<miku_core::ClusterId>,
    pub(crate) cluster_status_panel: Option<&'a mut ClusterStatusPanel>,
    pub(crate) cluster_role_binding_resource_panel: Option<&'a mut ClusterRoleBindingResourcePanel>,
    pub(crate) cluster_role_resource_panel: Option<&'a mut ClusterRoleResourcePanel>,
    pub(crate) config_map_resource_panel: Option<&'a mut ConfigMapResourcePanel>,
    pub(crate) cron_job_resource_panel: Option<&'a mut CronJobResourcePanel>,
    pub(crate) daemon_set_resource_panel: Option<&'a mut DaemonSetResourcePanel>,
    pub(crate) deployment_resource_panel: Option<&'a mut DeploymentResourcePanel>,
    pub(crate) endpoint_slice_resource_panel: Option<&'a mut EndpointSliceResourcePanel>,
    pub(crate) endpoints_resource_panel: Option<&'a mut EndpointsResourcePanel>,
    pub(crate) event_resource_panel: Option<&'a mut EventResourcePanel>,
    pub(crate) horizontal_pod_autoscaler_resource_panel:
        Option<&'a mut HorizontalPodAutoscalerResourcePanel>,
    pub(crate) ingress_class_resource_panel: Option<&'a mut IngressClassResourcePanel>,
    pub(crate) ingress_resource_panel: Option<&'a mut IngressResourcePanel>,
    pub(crate) job_resource_panel: Option<&'a mut JobResourcePanel>,
    pub(crate) lease_resource_panel: Option<&'a mut LeaseResourcePanel>,
    pub(crate) limit_range_resource_panel: Option<&'a mut LimitRangeResourcePanel>,
    pub(crate) mutating_webhook_configuration_resource_panel:
        Option<&'a mut MutatingWebhookConfigurationResourcePanel>,
    pub(crate) namespace_resource_panel: Option<&'a mut NamespaceResourcePanel>,
    pub(crate) network_policy_resource_panel: Option<&'a mut NetworkPolicyResourcePanel>,
    pub(crate) node_resource_panel: Option<&'a mut NodeResourcePanel>,
    pub(crate) persistent_volume_claim_resource_panel:
        Option<&'a mut PersistentVolumeClaimResourcePanel>,
    pub(crate) persistent_volume_resource_panel: Option<&'a mut PersistentVolumeResourcePanel>,
    pub(crate) pod_disruption_budget_resource_panel:
        Option<&'a mut PodDisruptionBudgetResourcePanel>,
    pub(crate) pod_resource_panel: Option<&'a mut PodResourcePanel>,
    pub(crate) priority_class_resource_panel: Option<&'a mut PriorityClassResourcePanel>,
    pub(crate) replica_set_resource_panel: Option<&'a mut ReplicaSetResourcePanel>,
    pub(crate) resource_quota_resource_panel: Option<&'a mut ResourceQuotaResourcePanel>,
    pub(crate) role_binding_resource_panel: Option<&'a mut RoleBindingResourcePanel>,
    pub(crate) role_resource_panel: Option<&'a mut RoleResourcePanel>,
    pub(crate) runtime_class_resource_panel: Option<&'a mut RuntimeClassResourcePanel>,
    pub(crate) secret_resource_panel: Option<&'a mut SecretResourcePanel>,
    pub(crate) service_account_resource_panel: Option<&'a mut ServiceAccountResourcePanel>,
    pub(crate) service_resource_panel: Option<&'a mut ServiceResourcePanel>,
    pub(crate) storage_class_resource_panel: Option<&'a mut StorageClassResourcePanel>,
    pub(crate) stateful_set_resource_panel: Option<&'a mut StatefulSetResourcePanel>,
    pub(crate) validating_webhook_configuration_resource_panel:
        Option<&'a mut ValidatingWebhookConfigurationResourcePanel>,
    pub(crate) custom_resources_panel: Option<&'a mut CustomResourcesPanel>,
    pub(crate) agent_panels: Option<&'a mut HashMap<usize, AgentPanel>>,
    pub(crate) agent_turn_requests: Vec<AgentTurnUiRequest>,
    pub(crate) agent_conversation_requests: Vec<AgentConversationUiRequest>,
    pub(crate) status_load_requests: Vec<ClusterStatusLoadRequest>,
    pub(crate) resource_load_requests: Vec<ResourceLoadRequest>,
    pub(crate) resource_watch_requests: Vec<ResourceWatchRequest>,
    pub(crate) resource_action_requests: Vec<ResourceActionRequest>,
    pub(crate) pod_log_requests: Vec<PodLogRequest>,
    pub(crate) pod_attach_requests: Vec<PodAttachRequest>,
    pub(crate) pod_attach_input_requests: Vec<PodAttachInputRequest>,
}

#[derive(Clone, Debug)]
pub(crate) struct AgentTurnUiRequest {
    pub(crate) request_id: u64,
    pub(crate) panel_id: usize,
    pub(crate) conversation_id: Option<String>,
    pub(crate) title: String,
    pub(crate) request: AgentTurnRequest,
}

#[derive(Clone, Debug)]
pub(crate) enum AgentConversationUiRequest {
    Load {
        panel_id: usize,
        conversation_id: String,
    },
    New {
        panel_id: usize,
    },
    Delete {
        panel_id: usize,
        conversation_id: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ClusterStatusLoadRequest {
    pub(crate) request_id: u64,
    pub(crate) cluster_id: miku_core::ClusterId,
}

#[derive(Debug, Default)]
pub(crate) struct AgentPanel {
    input: String,
    conversation_id: Option<String>,
    conversations: Vec<AgentConversationSummary>,
    messages: Vec<AgentMessage>,
    markdown_cache: CommonMarkCache,
    events: Vec<AgentEvent>,
    in_flight: Option<u64>,
    loading: bool,
    error: Option<String>,
    next_request_id: u64,
    load_conversation_requested: Option<String>,
    delete_conversation_requested: Option<String>,
    new_conversation_requested: bool,
}

#[derive(Debug, Default)]
pub(crate) struct ClusterStatusPanel {
    state: ClusterStatusPanelState,
    next_request_id: u64,
}

#[derive(Clone, Debug, Default, PartialEq)]
enum ClusterStatusPanelState {
    #[default]
    Idle,
    Loading {
        request_id: u64,
    },
    Ready {
        request_id: u64,
        report: ClusterStatusReport,
    },
    Failed {
        request_id: u64,
        error: String,
    },
}

impl TabViewer for AppTabViewer<'_> {
    type Tab = AppTab;

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        match tab {
            AppTab::Clusters => "Clusters",
            AppTab::Resources => "Resources",
            AppTab::Workspace(1) => "Workspace",
            AppTab::Workspace(id) => return format!("Workspace {id}").into(),
            AppTab::Resource(resource) => return resource.name.into(),
            AppTab::Agent(1) => "Agent",
            AppTab::Agent(id) => return format!("Agent {id}").into(),
        }
        .into()
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        match tab {
            AppTab::Clusters => {
                if ui
                    .add_sized(
                        [ui.available_width(), 30.0],
                        egui::Button::new("New Cluster"),
                    )
                    .clicked()
                {
                    self.new_cluster_requested = true;
                }
                ui.separator();

                if self.cluster_load_in_flight {
                    ui.label("Loading clusters...");
                } else if let Some(error) = self.cluster_load_error {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                } else if self.clusters.is_empty() {
                    ui.label("No clusters loaded yet.");
                } else {
                    for cluster in self.clusters {
                        self.show_cluster_card(ui, cluster);
                    }
                }
            }
            AppTab::Resources => self.show_resources(ui),
            AppTab::Workspace(_) => {
                match (
                    self.selected_cluster_id.as_ref(),
                    self.state.selected_cluster_name(),
                    self.cluster_status_panel.as_deref_mut(),
                ) {
                    (Some(cluster_id), Some(cluster_name), Some(panel)) => {
                        self.status_load_requests
                            .extend(panel.show(ui, cluster_id, cluster_name));
                    }
                    _ => {
                        ui.centered_and_justified(|ui| {
                            ui.label("Select a cluster to open its workspace.");
                        });
                    }
                }
            }
            AppTab::Resource(resource) => {
                if resource.name == "Cluster Role Bindings" {
                    if let Some(panel) = self.cluster_role_binding_resource_panel.as_deref_mut() {
                        let requests = panel.show(ui, self.selected_cluster_id.as_ref());
                        self.extend_resource_panel_requests(requests);
                    } else {
                        ui.centered_and_justified(|ui| {
                            ui.label("ClusterRoleBinding resource panel is unavailable.");
                        });
                    }
                } else if resource.name == "Cluster Roles" {
                    if let Some(panel) = self.cluster_role_resource_panel.as_deref_mut() {
                        let requests = panel.show(ui, self.selected_cluster_id.as_ref());
                        self.extend_resource_panel_requests(requests);
                    } else {
                        ui.centered_and_justified(|ui| {
                            ui.label("ClusterRole resource panel is unavailable.");
                        });
                    }
                } else if resource.name == "Config Maps" {
                    if let Some(panel) = self.config_map_resource_panel.as_deref_mut() {
                        let requests = panel.show(ui, self.selected_cluster_id.as_ref());
                        self.extend_resource_panel_requests(requests);
                    } else {
                        ui.centered_and_justified(|ui| {
                            ui.label("ConfigMap resource panel is unavailable.");
                        });
                    }
                } else if resource.name == "Cron Jobs" {
                    if let Some(panel) = self.cron_job_resource_panel.as_deref_mut() {
                        let requests = panel.show(ui, self.selected_cluster_id.as_ref());
                        self.extend_resource_panel_requests(requests);
                    } else {
                        ui.centered_and_justified(|ui| {
                            ui.label("CronJob resource panel is unavailable.");
                        });
                    }
                } else if resource.name == "Daemon Sets" {
                    if let Some(panel) = self.daemon_set_resource_panel.as_deref_mut() {
                        let requests = panel.show(ui, self.selected_cluster_id.as_ref());
                        self.extend_resource_panel_requests(requests);
                    } else {
                        ui.centered_and_justified(|ui| {
                            ui.label("DaemonSet resource panel is unavailable.");
                        });
                    }
                } else if resource.name == "Deployments" {
                    if let Some(panel) = self.deployment_resource_panel.as_deref_mut() {
                        let requests = panel.show(ui, self.selected_cluster_id.as_ref());
                        self.extend_resource_panel_requests(requests);
                    } else {
                        ui.centered_and_justified(|ui| {
                            ui.label("Deployment resource panel is unavailable.");
                        });
                    }
                } else if resource.name == "Events" {
                    if let Some(panel) = self.event_resource_panel.as_deref_mut() {
                        let requests = panel.show(ui, self.selected_cluster_id.as_ref());
                        self.extend_resource_panel_requests(requests);
                    } else {
                        ui.centered_and_justified(|ui| {
                            ui.label("Event resource panel is unavailable.");
                        });
                    }
                } else if resource.name == "Horizontal Pod Autoscalers" {
                    if let Some(panel) =
                        self.horizontal_pod_autoscaler_resource_panel.as_deref_mut()
                    {
                        let requests = panel.show(ui, self.selected_cluster_id.as_ref());
                        self.extend_resource_panel_requests(requests);
                    } else {
                        ui.centered_and_justified(|ui| {
                            ui.label("HorizontalPodAutoscaler resource panel is unavailable.");
                        });
                    }
                } else if resource.name == "Endpoint Slices" {
                    if let Some(panel) = self.endpoint_slice_resource_panel.as_deref_mut() {
                        let requests = panel.show(ui, self.selected_cluster_id.as_ref());
                        self.extend_resource_panel_requests(requests);
                    } else {
                        ui.centered_and_justified(|ui| {
                            ui.label("EndpointSlice resource panel is unavailable.");
                        });
                    }
                } else if resource.name == "Endpoints" {
                    if let Some(panel) = self.endpoints_resource_panel.as_deref_mut() {
                        let requests = panel.show(ui, self.selected_cluster_id.as_ref());
                        self.extend_resource_panel_requests(requests);
                    } else {
                        ui.centered_and_justified(|ui| {
                            ui.label("Endpoints resource panel is unavailable.");
                        });
                    }
                } else if resource.name == "Ingress Classes" {
                    if let Some(panel) = self.ingress_class_resource_panel.as_deref_mut() {
                        let requests = panel.show(ui, self.selected_cluster_id.as_ref());
                        self.extend_resource_panel_requests(requests);
                    } else {
                        ui.centered_and_justified(|ui| {
                            ui.label("IngressClass resource panel is unavailable.");
                        });
                    }
                } else if resource.name == "Ingresses" {
                    if let Some(panel) = self.ingress_resource_panel.as_deref_mut() {
                        let requests = panel.show(ui, self.selected_cluster_id.as_ref());
                        self.extend_resource_panel_requests(requests);
                    } else {
                        ui.centered_and_justified(|ui| {
                            ui.label("Ingress resource panel is unavailable.");
                        });
                    }
                } else if resource.name == "Jobs" {
                    if let Some(panel) = self.job_resource_panel.as_deref_mut() {
                        let requests = panel.show(ui, self.selected_cluster_id.as_ref());
                        self.extend_resource_panel_requests(requests);
                    } else {
                        ui.centered_and_justified(|ui| {
                            ui.label("Job resource panel is unavailable.");
                        });
                    }
                } else if resource.name == "Leases" {
                    if let Some(panel) = self.lease_resource_panel.as_deref_mut() {
                        let requests = panel.show(ui, self.selected_cluster_id.as_ref());
                        self.extend_resource_panel_requests(requests);
                    } else {
                        ui.centered_and_justified(|ui| {
                            ui.label("Lease resource panel is unavailable.");
                        });
                    }
                } else if resource.name == "Limit Ranges" {
                    if let Some(panel) = self.limit_range_resource_panel.as_deref_mut() {
                        let requests = panel.show(ui, self.selected_cluster_id.as_ref());
                        self.extend_resource_panel_requests(requests);
                    } else {
                        ui.centered_and_justified(|ui| {
                            ui.label("LimitRange resource panel is unavailable.");
                        });
                    }
                } else if resource.name == "Namespaces" {
                    if let Some(panel) = self.namespace_resource_panel.as_deref_mut() {
                        let requests = panel.show(ui, self.selected_cluster_id.as_ref());
                        self.extend_resource_panel_requests(requests);
                    } else {
                        ui.centered_and_justified(|ui| {
                            ui.label("Namespace resource panel is unavailable.");
                        });
                    }
                } else if resource.name == "Mutating Webhook Configurations" {
                    if let Some(panel) = self
                        .mutating_webhook_configuration_resource_panel
                        .as_deref_mut()
                    {
                        let requests = panel.show(ui, self.selected_cluster_id.as_ref());
                        self.extend_resource_panel_requests(requests);
                    } else {
                        ui.centered_and_justified(|ui| {
                            ui.label("MutatingWebhookConfiguration resource panel is unavailable.");
                        });
                    }
                } else if resource.name == "Nodes" {
                    if let Some(panel) = self.node_resource_panel.as_deref_mut() {
                        let requests = panel.show(ui, self.selected_cluster_id.as_ref());
                        self.extend_resource_panel_requests(requests);
                    } else {
                        ui.centered_and_justified(|ui| {
                            ui.label("Node resource panel is unavailable.");
                        });
                    }
                } else if resource.name == "Persistent Volume Claims" {
                    if let Some(panel) = self.persistent_volume_claim_resource_panel.as_deref_mut()
                    {
                        let requests = panel.show(ui, self.selected_cluster_id.as_ref());
                        self.extend_resource_panel_requests(requests);
                    } else {
                        ui.centered_and_justified(|ui| {
                            ui.label("PersistentVolumeClaim resource panel is unavailable.");
                        });
                    }
                } else if resource.name == "Persistent Volumes" {
                    if let Some(panel) = self.persistent_volume_resource_panel.as_deref_mut() {
                        let requests = panel.show(ui, self.selected_cluster_id.as_ref());
                        self.extend_resource_panel_requests(requests);
                    } else {
                        ui.centered_and_justified(|ui| {
                            ui.label("PersistentVolume resource panel is unavailable.");
                        });
                    }
                } else if resource.name == "Pod Disruption Budgets" {
                    if let Some(panel) = self.pod_disruption_budget_resource_panel.as_deref_mut() {
                        let requests = panel.show(ui, self.selected_cluster_id.as_ref());
                        self.extend_resource_panel_requests(requests);
                    } else {
                        ui.centered_and_justified(|ui| {
                            ui.label("PodDisruptionBudget resource panel is unavailable.");
                        });
                    }
                } else if resource.name == "Pods" {
                    if let Some(panel) = self.pod_resource_panel.as_deref_mut() {
                        let requests = panel.show(ui, self.selected_cluster_id.as_ref());
                        self.extend_resource_panel_requests(requests);
                    } else {
                        ui.centered_and_justified(|ui| {
                            ui.label("Pod resource panel is unavailable.");
                        });
                    }
                } else if resource.name == "Replica Sets" {
                    if let Some(panel) = self.replica_set_resource_panel.as_deref_mut() {
                        let requests = panel.show(ui, self.selected_cluster_id.as_ref());
                        self.extend_resource_panel_requests(requests);
                    } else {
                        ui.centered_and_justified(|ui| {
                            ui.label("ReplicaSet resource panel is unavailable.");
                        });
                    }
                } else if resource.name == "Priority Classes" {
                    if let Some(panel) = self.priority_class_resource_panel.as_deref_mut() {
                        let requests = panel.show(ui, self.selected_cluster_id.as_ref());
                        self.extend_resource_panel_requests(requests);
                    } else {
                        ui.centered_and_justified(|ui| {
                            ui.label("PriorityClass resource panel is unavailable.");
                        });
                    }
                } else if resource.name == "Resource Quotas" {
                    if let Some(panel) = self.resource_quota_resource_panel.as_deref_mut() {
                        let requests = panel.show(ui, self.selected_cluster_id.as_ref());
                        self.extend_resource_panel_requests(requests);
                    } else {
                        ui.centered_and_justified(|ui| {
                            ui.label("ResourceQuota resource panel is unavailable.");
                        });
                    }
                } else if resource.name == "Role Bindings" {
                    if let Some(panel) = self.role_binding_resource_panel.as_deref_mut() {
                        let requests = panel.show(ui, self.selected_cluster_id.as_ref());
                        self.extend_resource_panel_requests(requests);
                    } else {
                        ui.centered_and_justified(|ui| {
                            ui.label("RoleBinding resource panel is unavailable.");
                        });
                    }
                } else if resource.name == "Roles" {
                    if let Some(panel) = self.role_resource_panel.as_deref_mut() {
                        let requests = panel.show(ui, self.selected_cluster_id.as_ref());
                        self.extend_resource_panel_requests(requests);
                    } else {
                        ui.centered_and_justified(|ui| {
                            ui.label("Role resource panel is unavailable.");
                        });
                    }
                } else if resource.name == "Runtime Classes" {
                    if let Some(panel) = self.runtime_class_resource_panel.as_deref_mut() {
                        let requests = panel.show(ui, self.selected_cluster_id.as_ref());
                        self.extend_resource_panel_requests(requests);
                    } else {
                        ui.centered_and_justified(|ui| {
                            ui.label("RuntimeClass resource panel is unavailable.");
                        });
                    }
                } else if resource.name == "Secrets" {
                    if let Some(panel) = self.secret_resource_panel.as_deref_mut() {
                        let requests = panel.show(ui, self.selected_cluster_id.as_ref());
                        self.extend_resource_panel_requests(requests);
                    } else {
                        ui.centered_and_justified(|ui| {
                            ui.label("Secret resource panel is unavailable.");
                        });
                    }
                } else if resource.name == "Service Accounts" {
                    if let Some(panel) = self.service_account_resource_panel.as_deref_mut() {
                        let requests = panel.show(ui, self.selected_cluster_id.as_ref());
                        self.extend_resource_panel_requests(requests);
                    } else {
                        ui.centered_and_justified(|ui| {
                            ui.label("ServiceAccount resource panel is unavailable.");
                        });
                    }
                } else if resource.name == "Services" {
                    if let Some(panel) = self.service_resource_panel.as_deref_mut() {
                        let requests = panel.show(ui, self.selected_cluster_id.as_ref());
                        self.extend_resource_panel_requests(requests);
                    } else {
                        ui.centered_and_justified(|ui| {
                            ui.label("Service resource panel is unavailable.");
                        });
                    }
                } else if resource.name == "Storage Classes" {
                    if let Some(panel) = self.storage_class_resource_panel.as_deref_mut() {
                        let requests = panel.show(ui, self.selected_cluster_id.as_ref());
                        self.extend_resource_panel_requests(requests);
                    } else {
                        ui.centered_and_justified(|ui| {
                            ui.label("StorageClass resource panel is unavailable.");
                        });
                    }
                } else if resource.name == "Network Policies" {
                    if let Some(panel) = self.network_policy_resource_panel.as_deref_mut() {
                        let requests = panel.show(ui, self.selected_cluster_id.as_ref());
                        self.extend_resource_panel_requests(requests);
                    } else {
                        ui.centered_and_justified(|ui| {
                            ui.label("NetworkPolicy resource panel is unavailable.");
                        });
                    }
                } else if resource.name == "Stateful Sets" {
                    if let Some(panel) = self.stateful_set_resource_panel.as_deref_mut() {
                        let requests = panel.show(ui, self.selected_cluster_id.as_ref());
                        self.extend_resource_panel_requests(requests);
                    } else {
                        ui.centered_and_justified(|ui| {
                            ui.label("StatefulSet resource panel is unavailable.");
                        });
                    }
                } else if resource.name == "Validating Webhook Configurations" {
                    if let Some(panel) = self
                        .validating_webhook_configuration_resource_panel
                        .as_deref_mut()
                    {
                        let requests = panel.show(ui, self.selected_cluster_id.as_ref());
                        self.extend_resource_panel_requests(requests);
                    } else {
                        ui.centered_and_justified(|ui| {
                            ui.label(
                                "ValidatingWebhookConfiguration resource panel is unavailable.",
                            );
                        });
                    }
                } else if resource.name == "Custom Resources" {
                    if let Some(panel) = self.custom_resources_panel.as_deref_mut() {
                        let requests = panel.show(ui, self.selected_cluster_id.as_ref());
                        self.extend_resource_panel_requests(requests);
                    } else {
                        ui.centered_and_justified(|ui| {
                            ui.label("Custom resources panel is unavailable.");
                        });
                    }
                } else {
                    ui.centered_and_justified(|ui| {
                        ui.label(format!("{} panel is not implemented yet.", resource.name));
                    });
                }
            }
            AppTab::Agent(id) => {
                if let Some(agent_panels) = self.agent_panels.as_deref_mut() {
                    let panel = agent_panels.entry(*id).or_default();
                    if let Some(request) = panel.show(
                        ui,
                        *id,
                        self.selected_cluster_id.as_ref(),
                        self.state.selected_cluster_name(),
                        self.active_resource,
                    ) {
                        self.agent_turn_requests.push(request);
                    }
                    self.agent_conversation_requests
                        .extend(panel.take_conversation_requests(*id));
                } else {
                    ui.heading("Agent");
                    ui.separator();
                    ui.label("Agent panel is unavailable in this dock.");
                }
            }
        }
    }

    fn is_closeable(&self, tab: &Self::Tab) -> bool {
        self.closeable && !matches!(tab, AppTab::Clusters)
    }

    fn on_add(&mut self, _path: egui_dock::NodePath) {
        self.add_requested = self.add_tab.is_some();
    }

    fn allowed_in_windows(&self, _tab: &mut Self::Tab) -> bool {
        self.allow_windows
    }
}

impl AgentPanel {
    pub(crate) fn show(
        &mut self,
        ui: &mut egui::Ui,
        panel_id: usize,
        selected_cluster_id: Option<&miku_core::ClusterId>,
        selected_cluster_name: Option<&str>,
        active_resource: Option<ResourceNavItem>,
    ) -> Option<AgentTurnUiRequest> {
        let mut request = None;

        show_agent_header(ui, panel_id, self);
        ui.add_space(AGENT_SECTION_SPACING);

        let reserved_height = AGENT_COMPOSER_OUTER_HEIGHT
            + AGENT_MESSAGE_FRAME_VERTICAL_INSET
            + AGENT_SECTION_SPACING;
        let message_height = (ui.available_height() - reserved_height).max(120.0);
        show_agent_messages(ui, panel_id, message_height, self);

        let can_send = self.in_flight.is_none() && !self.input.trim().is_empty();
        if show_agent_composer(
            ui,
            &mut self.input,
            can_send,
            selected_cluster_name,
            active_resource,
        ) {
            let message = self.input.trim().to_owned();
            self.input.clear();
            self.error = None;
            self.events.clear();
            let request_id = self.next_request_id;
            self.next_request_id += 1;
            self.in_flight = Some(request_id);
            let selected_resource = active_resource.map(|resource| resource.name.to_owned());
            let history = self.messages.clone();
            self.messages.push(AgentMessage {
                role: AgentRole::User,
                content: message.clone(),
            });
            request = Some(AgentTurnUiRequest {
                request_id,
                panel_id,
                conversation_id: self.conversation_id.clone(),
                title: conversation_title(&message),
                request: AgentTurnRequest {
                    session_id: self
                        .conversation_id
                        .clone()
                        .unwrap_or_else(|| format!("agent-{panel_id}")),
                    message,
                    context: AgentContext {
                        cluster_id: selected_cluster_id.cloned(),
                        cluster_name: selected_cluster_name.map(str::to_owned),
                        selected_resource,
                        namespace: None,
                    },
                    history,
                },
            });
        }

        request
    }

    pub(crate) fn apply_result(
        &mut self,
        request_id: u64,
        result: Result<AgentTurnUiResponse, String>,
    ) {
        if self.in_flight != Some(request_id) {
            return;
        }
        self.in_flight = None;

        match result {
            Ok(response) => {
                self.conversation_id = Some(response.conversation_id);
                self.events = response.response.events;
                self.messages.push(response.response.message);
                self.error = None;
            }
            Err(error) => {
                self.error = Some(error);
            }
        }
    }

    pub(crate) fn apply_conversations(
        &mut self,
        conversations: Vec<AgentConversationSummary>,
        result: Result<Option<AgentConversationUiData>, String>,
    ) {
        self.loading = false;
        self.conversations = conversations;
        match result {
            Ok(Some(conversation)) => {
                self.conversation_id = Some(conversation.summary.id);
                self.messages = conversation.messages;
                self.events.clear();
                self.error = None;
            }
            Ok(None) => {
                self.conversation_id = None;
                self.messages.clear();
                self.events.clear();
                self.error = None;
            }
            Err(error) => {
                self.error = Some(error);
            }
        }
    }

    pub(crate) fn set_loading(&mut self) {
        self.loading = true;
    }

    pub(crate) fn set_error(&mut self, error: String) {
        self.loading = false;
        self.error = Some(error);
    }

    pub(crate) fn start_new_conversation(&mut self) {
        self.conversation_id = None;
        self.messages.clear();
        self.events.clear();
        self.error = None;
        self.input.clear();
        self.loading = false;
    }

    fn take_conversation_requests(&mut self, panel_id: usize) -> Vec<AgentConversationUiRequest> {
        let mut requests = Vec::new();
        if self.new_conversation_requested {
            self.new_conversation_requested = false;
            requests.push(AgentConversationUiRequest::New { panel_id });
        }
        if let Some(conversation_id) = self.load_conversation_requested.take() {
            requests.push(AgentConversationUiRequest::Load {
                panel_id,
                conversation_id,
            });
        }
        if let Some(conversation_id) = self.delete_conversation_requested.take() {
            requests.push(AgentConversationUiRequest::Delete {
                panel_id,
                conversation_id,
            });
        }
        requests
    }
}

#[derive(Clone, Debug)]
pub(crate) struct AgentTurnUiResponse {
    pub(crate) conversation_id: String,
    pub(crate) response: AgentTurnResponse,
}

#[derive(Clone, Debug)]
pub(crate) struct AgentConversationUiData {
    pub(crate) summary: AgentConversationSummary,
    pub(crate) messages: Vec<AgentMessage>,
}

fn show_agent_header(ui: &mut egui::Ui, panel_id: usize, panel: &mut AgentPanel) {
    let (status_icon, status_text, status_color) = if panel.loading {
        (
            egui_phosphor::regular::CIRCLE_NOTCH,
            "Loading",
            ui.visuals().hyperlink_color,
        )
    } else if panel.in_flight.is_some() {
        (
            egui_phosphor::regular::CIRCLE_NOTCH,
            "Thinking",
            ui.visuals().hyperlink_color,
        )
    } else if panel.error.is_some() {
        (
            egui_phosphor::regular::WARNING_CIRCLE,
            "Needs attention",
            ui.visuals().error_fg_color,
        )
    } else {
        (
            egui_phosphor::regular::SPARKLE,
            "Ready",
            ui.visuals().weak_text_color(),
        )
    };

    egui::Frame::new()
        .fill(ui.visuals().widgets.inactive.bg_fill)
        .stroke(ui.visuals().widgets.inactive.bg_stroke)
        .corner_radius(egui::CornerRadius::same(6))
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(egui_phosphor::regular::SPARKLE)
                        .color(ui.visuals().hyperlink_color),
                );
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new("Miku Agent").strong());
                    ui.label(
                        egui::RichText::new(current_conversation_title(panel))
                            .small()
                            .color(ui.visuals().weak_text_color()),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new(status_text).small().color(status_color));
                    ui.label(egui::RichText::new(status_icon).color(status_color));
                    show_agent_conversation_menu(ui, panel_id, panel);
                });
            });
        });
}

fn show_agent_conversation_menu(ui: &mut egui::Ui, panel_id: usize, panel: &mut AgentPanel) {
    ui.menu_button(egui_phosphor::regular::CHATS, |ui| {
        if ui
            .button(format!("{} New chat", egui_phosphor::regular::PLUS))
            .clicked()
        {
            panel.new_conversation_requested = true;
            ui.close();
        }
        if let Some(conversation_id) = panel.conversation_id.clone()
            && ui
                .button(format!("{} Delete current", egui_phosphor::regular::TRASH))
                .clicked()
        {
            panel.delete_conversation_requested = Some(conversation_id);
            ui.close();
        }
        if !panel.conversations.is_empty() {
            ui.separator();
        }
        for conversation in &panel.conversations {
            let selected = panel.conversation_id.as_deref() == Some(conversation.id.as_str());
            let label = if selected {
                format!("{} {}", egui_phosphor::regular::CHECK, conversation.title)
            } else {
                conversation.title.clone()
            };
            if ui.selectable_label(selected, label).clicked() {
                panel.load_conversation_requested = Some(conversation.id.clone());
                ui.close();
            }
        }
    })
    .response
    .on_hover_text(format!("Agent conversations {panel_id}"));
}

fn current_conversation_title(panel: &AgentPanel) -> &str {
    panel
        .conversation_id
        .as_ref()
        .and_then(|id| {
            panel
                .conversations
                .iter()
                .find(|conversation| conversation.id == *id)
        })
        .map(|conversation| conversation.title.as_str())
        .unwrap_or("Kubernetes assistant")
}

fn context_chip(ui: &mut egui::Ui, icon: &str, text: &str) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.label(
            egui::RichText::new(icon)
                .small()
                .color(ui.visuals().weak_text_color()),
        );
        ui.label(egui::RichText::new(text).small());
    });
}

fn show_agent_context_bar(
    ui: &mut egui::Ui,
    selected_cluster_name: Option<&str>,
    active_resource: Option<ResourceNavItem>,
) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 14.0;
        context_chip(
            ui,
            egui_phosphor::regular::TREE_STRUCTURE,
            selected_cluster_name.unwrap_or("No cluster selected"),
        );
        context_chip(
            ui,
            egui_phosphor::regular::CUBE,
            active_resource
                .map(|resource| resource.name)
                .unwrap_or("No resource"),
        );
    });
}

fn show_agent_messages(
    ui: &mut egui::Ui,
    panel_id: usize,
    message_height: f32,
    panel: &mut AgentPanel,
) {
    egui::Frame::new()
        .fill(ui.visuals().panel_fill)
        .stroke(egui::Stroke::new(
            1.0,
            ui.visuals().widgets.noninteractive.bg_stroke.color,
        ))
        .corner_radius(egui::CornerRadius::same(6))
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt(("agent_messages", panel_id))
                .max_height(message_height)
                .min_scrolled_height(message_height)
                .auto_shrink([false, false])
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    if panel.messages.is_empty() {
                        if panel.loading {
                            show_agent_loading_state(ui);
                        } else {
                            show_agent_empty_state(ui);
                        }
                    }

                    for index in 0..panel.messages.len() {
                        let message = panel.messages[index].clone();
                        show_agent_message(ui, &message, &mut panel.markdown_cache);
                        ui.add_space(8.0);
                    }

                    if !panel.events.is_empty() {
                        show_agent_tool_activity(ui, &panel.events);
                        ui.add_space(8.0);
                    }

                    if let Some(error) = &panel.error {
                        show_agent_error(ui, error);
                    }

                    if panel.in_flight.is_some() {
                        show_agent_thinking(ui);
                    }
                });
        });
}

fn show_agent_loading_state(ui: &mut egui::Ui) {
    ui.add_space(24.0);
    ui.vertical_centered(|ui| {
        ui.add(egui::Spinner::new().size(18.0));
        ui.label(
            egui::RichText::new("Loading conversation")
                .small()
                .color(ui.visuals().weak_text_color()),
        );
    });
}

fn show_agent_empty_state(ui: &mut egui::Ui) {
    ui.add_space(24.0);
    ui.vertical_centered(|ui| {
        ui.label(
            egui::RichText::new(egui_phosphor::regular::SPARKLE)
                .size(22.0)
                .color(ui.visuals().hyperlink_color),
        );
        ui.label(egui::RichText::new("Ask Miku about this cluster").strong());
        ui.label(
            egui::RichText::new("Check status, explain resources, or read pod logs.")
                .small()
                .color(ui.visuals().weak_text_color()),
        );
    });
}

fn show_agent_message(
    ui: &mut egui::Ui,
    message: &AgentMessage,
    markdown_cache: &mut CommonMarkCache,
) {
    match message.role {
        AgentRole::User => show_user_message(ui, message),
        AgentRole::Assistant => show_assistant_message(ui, message, markdown_cache),
        AgentRole::Tool => show_tool_message(ui, message),
    }
}

fn show_user_message(ui: &mut egui::Ui, message: &AgentMessage) {
    let width = agent_bubble_width(ui);
    ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
        message_bubble(
            ui,
            &message.content,
            MessageBubbleStyle {
                label: "You",
                icon: egui_phosphor::regular::USER,
                max_width: width,
                fill: ui.visuals().selection.bg_fill,
                stroke: ui.visuals().selection.stroke,
                accent: ui.visuals().selection.stroke.color,
            },
        );
    });
}

fn show_assistant_message(
    ui: &mut egui::Ui,
    message: &AgentMessage,
    markdown_cache: &mut CommonMarkCache,
) {
    markdown_message_bubble(
        ui,
        &message.content,
        markdown_cache,
        MessageBubbleStyle {
            label: "Miku",
            icon: egui_phosphor::regular::SPARKLE,
            max_width: agent_bubble_width(ui),
            fill: ui.visuals().extreme_bg_color,
            stroke: egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
            accent: ui.visuals().hyperlink_color,
        },
    );
}

fn show_tool_message(ui: &mut egui::Ui, message: &AgentMessage) {
    message_bubble(
        ui,
        &message.content,
        MessageBubbleStyle {
            label: "Tool",
            icon: egui_phosphor::regular::WRENCH,
            max_width: agent_bubble_width(ui),
            fill: ui.visuals().widgets.inactive.bg_fill,
            stroke: ui.visuals().widgets.inactive.bg_stroke,
            accent: ui.visuals().weak_text_color(),
        },
    );
}

struct MessageBubbleStyle<'a> {
    label: &'a str,
    icon: &'a str,
    max_width: f32,
    fill: egui::Color32,
    stroke: egui::Stroke,
    accent: egui::Color32,
}

fn message_bubble(ui: &mut egui::Ui, content: &str, style: MessageBubbleStyle) {
    egui::Frame::new()
        .fill(style.fill)
        .stroke(style.stroke)
        .corner_radius(egui::CornerRadius::same(6))
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_max_width(style.max_width);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(style.icon).small().color(style.accent));
                ui.label(
                    egui::RichText::new(style.label)
                        .small()
                        .strong()
                        .color(style.accent),
                );
            });
            ui.add(
                egui::Label::new(egui::RichText::new(content).color(ui.visuals().text_color()))
                    .wrap(),
            );
        });
}

fn markdown_message_bubble(
    ui: &mut egui::Ui,
    content: &str,
    markdown_cache: &mut CommonMarkCache,
    style: MessageBubbleStyle,
) {
    egui::Frame::new()
        .fill(style.fill)
        .stroke(style.stroke)
        .corner_radius(egui::CornerRadius::same(6))
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_max_width(style.max_width);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(style.icon).small().color(style.accent));
                ui.label(
                    egui::RichText::new(style.label)
                        .small()
                        .strong()
                        .color(style.accent),
                );
            });
            CommonMarkViewer::new()
                .default_width(Some(style.max_width as usize))
                .show(ui, markdown_cache, content);
        });
}

fn agent_bubble_width(ui: &egui::Ui) -> f32 {
    (ui.available_width() * 0.82).max(120.0)
}

fn show_agent_tool_activity(ui: &mut egui::Ui, events: &[AgentEvent]) {
    egui::CollapsingHeader::new(format!("{} Tool activity", egui_phosphor::regular::WRENCH))
        .default_open(false)
        .show(ui, |ui| {
            for event in events {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(agent_event_icon(event))
                            .color(agent_event_color(ui, event)),
                    );
                    ui.label(
                        egui::RichText::new(format_agent_event(event))
                            .small()
                            .color(ui.visuals().weak_text_color()),
                    );
                });
            }
        });
}

fn show_agent_error(ui: &mut egui::Ui, error: &str) {
    status_frame(ui, ui.visuals().error_fg_color, |ui| {
        ui.label(
            egui::RichText::new(egui_phosphor::regular::WARNING_CIRCLE)
                .color(ui.visuals().error_fg_color),
        );
        ui.label(egui::RichText::new(error).color(ui.visuals().error_fg_color));
    });
}

fn show_agent_thinking(ui: &mut egui::Ui) {
    status_frame(ui, ui.visuals().hyperlink_color, |ui| {
        ui.add(egui::Spinner::new().size(14.0));
        ui.label(
            egui::RichText::new("Miku is thinking...")
                .small()
                .color(ui.visuals().weak_text_color()),
        );
    });
}

fn status_frame<R>(
    ui: &mut egui::Ui,
    color: egui::Color32,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) {
    egui::Frame::new()
        .fill(ui.visuals().widgets.inactive.bg_fill)
        .stroke(egui::Stroke::new(1.0, color))
        .corner_radius(egui::CornerRadius::same(6))
        .inner_margin(egui::Margin::symmetric(8, 6))
        .show(ui, |ui| {
            ui.horizontal(add_contents);
        });
}

fn show_agent_composer(
    ui: &mut egui::Ui,
    input: &mut String,
    can_send: bool,
    selected_cluster_name: Option<&str>,
    active_resource: Option<ResourceNavItem>,
) -> bool {
    let mut send_clicked = false;
    egui::Frame::new()
        .fill(ui.visuals().extreme_bg_color)
        .stroke(egui::Stroke::new(
            1.0,
            ui.visuals().widgets.noninteractive.bg_stroke.color,
        ))
        .corner_radius(egui::CornerRadius::same(6))
        .inner_margin(egui::Margin::symmetric(8, 8))
        .show(ui, |ui| {
            ui.set_min_height(128.0);
            let input_response = ui.add(
                egui::TextEdit::multiline(input)
                    .desired_rows(5)
                    .desired_width(ui.available_width())
                    .return_key(egui::KeyboardShortcut::new(
                        egui::Modifiers::SHIFT,
                        egui::Key::Enter,
                    ))
                    .hint_text("Ask about the selected cluster..."),
            );
            let enter_pressed = input_response.has_focus()
                && ui.input(|input| input.key_pressed(egui::Key::Enter) && !input.modifiers.shift);
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                show_agent_context_bar(ui, selected_cluster_name, active_resource);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let send_text = egui::RichText::new(egui_phosphor::regular::PAPER_PLANE_TILT)
                        .color(if can_send {
                            ui.visuals().hyperlink_color
                        } else {
                            ui.visuals().weak_text_color()
                        });
                    send_clicked = ui
                        .add_enabled(can_send, egui::Button::new(send_text))
                        .on_hover_text("Send")
                        .clicked();
                });
            });
            if can_send && enter_pressed {
                send_clicked = true;
            }
        });
    send_clicked
}

fn conversation_title(message: &str) -> String {
    let mut title = message.chars().take(48).collect::<String>();
    if title.trim().is_empty() {
        title = "New conversation".to_owned();
    }
    title
}

fn agent_event_icon(event: &AgentEvent) -> &'static str {
    match event {
        AgentEvent::ToolStarted { .. } => egui_phosphor::regular::CIRCLE_NOTCH,
        AgentEvent::ToolFinished { .. } => egui_phosphor::regular::CHECK_CIRCLE,
        AgentEvent::ToolFailed { .. } => egui_phosphor::regular::WARNING_CIRCLE,
        AgentEvent::Completed { .. } => egui_phosphor::regular::CHECK,
    }
}

fn agent_event_color(ui: &egui::Ui, event: &AgentEvent) -> egui::Color32 {
    match event {
        AgentEvent::ToolStarted { .. } => ui.visuals().hyperlink_color,
        AgentEvent::ToolFinished { .. } | AgentEvent::Completed { .. } => {
            ui.visuals().weak_text_color()
        }
        AgentEvent::ToolFailed { .. } => ui.visuals().error_fg_color,
    }
}

fn format_agent_event(event: &AgentEvent) -> String {
    match event {
        AgentEvent::ToolStarted { name, .. } => format!("Started {name}"),
        AgentEvent::ToolFinished { name, .. } => format!("Finished {name}"),
        AgentEvent::ToolFailed { name, error } => format!("{name} failed: {error}"),
        AgentEvent::Completed { status, summary } => format!("{status:?}: {summary}"),
    }
}

impl ClusterStatusPanel {
    pub(crate) fn show(
        &mut self,
        ui: &mut egui::Ui,
        cluster_id: &miku_core::ClusterId,
        cluster_name: &str,
    ) -> Vec<ClusterStatusLoadRequest> {
        let mut requests = Vec::new();
        if let Some(request) = self.request_status_if_idle(cluster_id.clone()) {
            requests.push(request);
        }

        ui.horizontal(|ui| {
            ui.set_width(ui.available_width());
            ui.heading(cluster_name);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button(format!(
                        "{} Refresh",
                        egui_phosphor::regular::ARROW_CLOCKWISE
                    ))
                    .clicked()
                {
                    requests.push(self.request_status(cluster_id.clone()));
                }
                match &self.state {
                    ClusterStatusPanelState::Loading { .. } => {
                        ui.add(egui::Spinner::new().size(16.0));
                        ui.label("Loading status");
                    }
                    ClusterStatusPanelState::Ready { .. } => {
                        ui.label(
                            egui::RichText::new(format!(
                                "{} Current",
                                egui_phosphor::regular::CHECK_CIRCLE
                            ))
                            .color(ui.visuals().hyperlink_color),
                        );
                    }
                    ClusterStatusPanelState::Failed { .. } => {
                        ui.label(
                            egui::RichText::new(format!(
                                "{} Status unavailable",
                                egui_phosphor::regular::WARNING_CIRCLE
                            ))
                            .color(ui.visuals().error_fg_color),
                        );
                    }
                    ClusterStatusPanelState::Idle => {}
                }
            });
        });
        ui.separator();
        let page_rect = ui.available_rect_before_wrap();

        match &self.state {
            ClusterStatusPanelState::Idle => {
                show_centered_in_rect(ui, page_rect, |ui| {
                    ui.label("Preparing cluster status.");
                });
            }
            ClusterStatusPanelState::Loading { .. } => {
                show_centered_loading_row(ui, page_rect, "Loading cluster status...");
            }
            ClusterStatusPanelState::Ready { report, .. } => show_status_report(ui, report),
            ClusterStatusPanelState::Failed { error, .. } => {
                let error = error.clone();
                ui.vertical_centered(|ui| {
                    ui.colored_label(ui.visuals().error_fg_color, &error);
                    if ui.button("Retry").clicked() {
                        requests.push(self.request_status(cluster_id.clone()));
                    }
                });
            }
        }

        requests
    }

    pub(crate) fn apply_result(
        &mut self,
        request: &ClusterStatusLoadRequest,
        result: Result<ClusterStatusReport, String>,
    ) {
        if !matches!(
            self.state,
            ClusterStatusPanelState::Loading { request_id } if request_id == request.request_id
        ) {
            return;
        }

        self.state = match result {
            Ok(report) => ClusterStatusPanelState::Ready {
                request_id: request.request_id,
                report,
            },
            Err(error) => ClusterStatusPanelState::Failed {
                request_id: request.request_id,
                error,
            },
        };
    }

    fn request_status(&mut self, cluster_id: miku_core::ClusterId) -> ClusterStatusLoadRequest {
        self.next_request_id += 1;
        let request = ClusterStatusLoadRequest {
            request_id: self.next_request_id,
            cluster_id,
        };
        self.state = ClusterStatusPanelState::Loading {
            request_id: request.request_id,
        };
        request
    }

    fn request_status_if_idle(
        &mut self,
        cluster_id: miku_core::ClusterId,
    ) -> Option<ClusterStatusLoadRequest> {
        matches!(self.state, ClusterStatusPanelState::Idle).then(|| self.request_status(cluster_id))
    }
}

fn show_centered_in_rect<R>(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    let top_space = (rect.height() * 0.5 - 16.0).max(0.0);
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Center)),
        |ui| {
            ui.add_space(top_space);
            add_contents(ui)
        },
    )
}

fn show_centered_loading_row(ui: &mut egui::Ui, rect: egui::Rect, text: &str) {
    const SPINNER_SIZE: f32 = 18.0;

    let font_id = egui::TextStyle::Body.resolve(ui.style());
    let text_size = ui
        .painter()
        .layout_no_wrap(text.to_owned(), font_id, ui.visuals().text_color())
        .size();
    let row_size = egui::vec2(
        SPINNER_SIZE + ui.spacing().item_spacing.x + text_size.x,
        SPINNER_SIZE.max(text_size.y),
    );
    let row_rect = egui::Rect::from_center_size(rect.center(), row_size);

    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(row_rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
        |ui| {
            ui.set_min_size(row_size);
            ui.add(egui::Spinner::new().size(SPINNER_SIZE));
            ui.label(text);
        },
    );
}

impl AppTabViewer<'_> {
    fn extend_resource_panel_requests(&mut self, requests: ResourcePanelRequests) {
        self.resource_load_requests.extend(requests.loads);
        self.resource_watch_requests.extend(requests.watches);
        self.resource_action_requests.extend(requests.actions);
        self.pod_log_requests.extend(requests.logs);
        self.pod_attach_requests.extend(requests.attaches);
        self.pod_attach_input_requests
            .extend(requests.attach_inputs);
    }

    fn show_cluster_card(&mut self, ui: &mut egui::Ui, cluster: &ClusterSummary) {
        let selected = self.state.selected_cluster_id() == Some(&cluster.id);
        let connection_state = self
            .cluster_connection_states
            .get(&cluster.id)
            .unwrap_or(&ClusterConnectionState::Idle);
        let visuals = ui.visuals();
        let fill = if selected {
            visuals.selection.bg_fill
        } else {
            visuals.widgets.inactive.bg_fill
        };
        let stroke = if selected {
            visuals.selection.stroke
        } else {
            visuals.widgets.inactive.bg_stroke
        };

        let frame = egui::Frame::new()
            .fill(fill)
            .stroke(stroke)
            .corner_radius(egui::CornerRadius::same(6))
            .inner_margin(egui::Margin::symmetric(10, 8));
        let response = frame
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(&cluster.name).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        show_connection_state(ui, connection_state);
                    });
                });
            })
            .response
            .interact(egui::Sense::click());

        if response.clicked() || response.double_clicked() {
            self.selected_cluster = Some(cluster.clone());
        }

        if response.secondary_clicked() {
            self.selected_cluster = Some(cluster.clone());
        }

        response.context_menu(|ui| {
            if ui.button("Select").clicked() {
                self.selected_cluster = Some(cluster.clone());
                ui.close();
            }
        });

        ui.add_space(6.0);
    }

    fn show_resources(&mut self, ui: &mut egui::Ui) {
        for category in RESOURCE_CATEGORIES {
            ui.horizontal(|ui| {
                let text_color = ui.visuals().weak_text_color();
                ui.label(egui::RichText::new(category.icon).color(text_color));
                ui.label(
                    egui::RichText::new(category.name)
                        .color(text_color)
                        .strong(),
                );
            });
            ui.separator();

            for resource in category.items {
                let selected = self.active_resource == Some(*resource);
                let response = ui.selectable_label(selected, resource.name);
                if response.clicked() {
                    self.selected_resource = Some(*resource);
                }
            }
        }
    }
}

fn show_connection_state(ui: &mut egui::Ui, state: &ClusterConnectionState) {
    match state {
        ClusterConnectionState::Idle => {
            ui.label(
                egui::RichText::new(egui_phosphor::regular::CIRCLE)
                    .color(ui.visuals().weak_text_color()),
            )
            .on_hover_text("Not initialized");
        }
        ClusterConnectionState::Initializing => {
            ui.add(egui::Spinner::new().size(16.0))
                .on_hover_text("Initializing cluster");
        }
        ClusterConnectionState::Ready { info } => {
            let color = ui.visuals().hyperlink_color;
            let label = if info.version.is_empty() {
                egui_phosphor::regular::CHECK_CIRCLE.to_owned()
            } else {
                format!("{} {}", egui_phosphor::regular::CHECK_CIRCLE, info.version)
            };
            let hover = info
                .platform
                .as_ref()
                .map(|platform| format!("Connected\nPlatform: {platform}"))
                .unwrap_or_else(|| "Connected".to_owned());
            ui.label(egui::RichText::new(label).color(color))
                .on_hover_text(hover);
        }
        ClusterConnectionState::Failed { error } => {
            ui.label(
                egui::RichText::new(egui_phosphor::regular::WARNING_CIRCLE)
                    .color(ui.visuals().error_fg_color),
            )
            .on_hover_text(error);
        }
    }
}

fn show_status_report(ui: &mut egui::Ui, report: &ClusterStatusReport) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            metric(ui, "Version", &report.overview.version);
            metric(
                ui,
                "Platform",
                report.overview.platform.as_deref().unwrap_or("unknown"),
            );
            metric(ui, "Namespaces", &report.overview.namespaces.to_string());
            metric(
                ui,
                "Nodes",
                &format!("{}/{}", report.overview.ready_nodes, report.overview.nodes),
            );
            metric(ui, "Pods", &report.overview.pods.to_string());
            metric(
                ui,
                "Unhealthy Pods",
                &report.overview.unhealthy_pods.to_string(),
            );
        });

        ui.add_space(12.0);
        ui.columns(2, |columns| {
            show_health_conditions(&mut columns[0], &report.conditions);
            show_workloads(&mut columns[1], &report.workloads);
        });

        ui.add_space(12.0);
        show_recent_events(ui, &report.recent_events);
    });
}

fn metric(ui: &mut egui::Ui, label: &str, value: &str) {
    egui::Frame::new()
        .fill(ui.visuals().widgets.inactive.bg_fill)
        .stroke(ui.visuals().widgets.inactive.bg_stroke)
        .corner_radius(egui::CornerRadius::same(6))
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_min_width(110.0);
            ui.vertical(|ui| {
                ui.label(egui::RichText::new(label).color(ui.visuals().weak_text_color()));
                ui.label(egui::RichText::new(value).strong());
            });
        });
}

fn show_health_conditions(ui: &mut egui::Ui, conditions: &[miku_api::ClusterStatusCondition]) {
    ui.heading("Cluster Health");
    ui.separator();
    for condition in conditions {
        ui.horizontal(|ui| {
            let (icon, color) = severity_icon(ui, &condition.severity);
            ui.label(egui::RichText::new(icon).color(color));
            ui.vertical(|ui| {
                ui.label(egui::RichText::new(&condition.name).strong());
                ui.label(&condition.status);
                ui.label(
                    egui::RichText::new(&condition.message).color(ui.visuals().weak_text_color()),
                );
            });
        });
        ui.add_space(8.0);
    }
}

fn show_workloads(ui: &mut egui::Ui, workloads: &ClusterStatusWorkloadSummary) {
    ui.heading("Workloads");
    ui.separator();
    workload_row(ui, "Pods", workloads.pods);
    workload_row(ui, "Deployments", workloads.deployments);
    workload_row(ui, "Services", workloads.services);
    workload_row(ui, "Config Maps", workloads.config_maps);
    workload_row(ui, "Secrets", workloads.secrets);
}

fn workload_row(ui: &mut egui::Ui, label: &str, value: usize) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(egui::RichText::new(value.to_string()).strong());
        });
    });
}

fn show_recent_events(ui: &mut egui::Ui, events: &[miku_api::ClusterStatusEventSummary]) {
    ui.heading("Recent Events");
    ui.separator();
    if events.is_empty() {
        ui.label(egui::RichText::new("No recent events.").color(ui.visuals().weak_text_color()));
        return;
    }

    egui::ScrollArea::horizontal()
        .id_salt("cluster_status_recent_events_scroll")
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.set_min_width(860.0);
            egui::Grid::new("cluster_status_recent_events")
                .num_columns(5)
                .striped(true)
                .show(ui, |ui| {
                    ui.strong("Namespace");
                    ui.strong("Object");
                    ui.strong("Reason");
                    ui.strong("Type");
                    ui.strong("Message");
                    ui.end_row();

                    for event in events {
                        ui.label(event.namespace.as_deref().unwrap_or("-"));
                        ui.label(&event.involved_object);
                        ui.label(&event.reason);
                        ui.label(&event.event_type);
                        ui.label(&event.message);
                        ui.end_row();
                    }
                });
        });
}

fn severity_icon(ui: &egui::Ui, severity: &ClusterStatusSeverity) -> (&'static str, egui::Color32) {
    match severity {
        ClusterStatusSeverity::Ok => (
            egui_phosphor::regular::CHECK_CIRCLE,
            ui.visuals().hyperlink_color,
        ),
        ClusterStatusSeverity::Warning => (
            egui_phosphor::regular::WARNING_CIRCLE,
            ui.visuals().warn_fg_color,
        ),
        ClusterStatusSeverity::Critical => (
            egui_phosphor::regular::X_CIRCLE,
            ui.visuals().error_fg_color,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_panel_requests_status_once_when_idle() {
        let mut panel = ClusterStatusPanel::default();
        let cluster_id = miku_core::ClusterId::new("local");

        let first = panel.request_status_if_idle(cluster_id.clone());
        let second = panel.request_status_if_idle(cluster_id);

        assert!(first.is_some());
        assert!(second.is_none());
        assert!(matches!(
            panel.state,
            ClusterStatusPanelState::Loading { request_id: 1 }
        ));
    }

    #[test]
    fn status_panel_applies_ready_result_for_matching_request() {
        let mut panel = ClusterStatusPanel::default();
        let request = panel.request_status(miku_core::ClusterId::new("local"));

        panel.apply_result(&request, Ok(status_report()));

        assert!(matches!(
            &panel.state,
            ClusterStatusPanelState::Ready { report, .. }
                if report.overview.version == "v1.35.0"
        ));
    }

    #[test]
    fn status_panel_applies_failed_result_for_matching_request() {
        let mut panel = ClusterStatusPanel::default();
        let request = panel.request_status(miku_core::ClusterId::new("local"));

        panel.apply_result(&request, Err("forbidden".to_owned()));

        assert!(matches!(
            &panel.state,
            ClusterStatusPanelState::Failed { error, .. } if error == "forbidden"
        ));
    }

    #[test]
    fn status_panel_ignores_stale_results() {
        let mut panel = ClusterStatusPanel::default();
        let stale = panel.request_status(miku_core::ClusterId::new("local"));
        let current = panel.request_status(miku_core::ClusterId::new("local"));

        panel.apply_result(&stale, Ok(status_report()));

        assert!(matches!(
            panel.state,
            ClusterStatusPanelState::Loading { request_id } if request_id == current.request_id
        ));
    }

    #[test]
    fn resource_panel_requests_collect_non_pod_actions() {
        let cluster_id = miku_core::ClusterId::new("local");
        let mut viewer = test_tab_viewer(cluster_id.clone());
        let request = ResourceActionRequest {
            request_id: 7,
            cluster_id,
            kind: crate::resource_panel::ResourceActionKind::ApplyResource {
                resource: miku_core::ResourceRef::grouped("batch", "v1", "cronjobs"),
                namespace: Some("default".to_owned()),
                name: "example-cron-job".to_owned(),
                manifest: serde_json::json!({"kind": "CronJob"}),
            },
        };

        viewer.extend_resource_panel_requests(crate::resource_panel::ResourcePanelRequests {
            actions: vec![request.clone()],
            ..Default::default()
        });

        assert_eq!(viewer.resource_action_requests, vec![request]);
    }

    fn test_tab_viewer(cluster_id: miku_core::ClusterId) -> AppTabViewer<'static> {
        AppTabViewer {
            state: Box::leak(Box::new(AppState::new(crate::state::RuntimeMode::Native))),
            clusters: &[],
            cluster_connection_states: Box::leak(Box::new(HashMap::new())),
            cluster_load_in_flight: false,
            cluster_load_error: None,
            closeable: false,
            allow_windows: false,
            add_tab: None,
            add_requested: false,
            new_cluster_requested: false,
            selected_cluster: None,
            active_resource: None,
            selected_resource: None,
            selected_cluster_id: Some(cluster_id),
            cluster_status_panel: None,
            cluster_role_binding_resource_panel: None,
            cluster_role_resource_panel: None,
            config_map_resource_panel: None,
            cron_job_resource_panel: None,
            daemon_set_resource_panel: None,
            deployment_resource_panel: None,
            endpoint_slice_resource_panel: None,
            endpoints_resource_panel: None,
            event_resource_panel: None,
            horizontal_pod_autoscaler_resource_panel: None,
            ingress_class_resource_panel: None,
            ingress_resource_panel: None,
            job_resource_panel: None,
            lease_resource_panel: None,
            limit_range_resource_panel: None,
            mutating_webhook_configuration_resource_panel: None,
            namespace_resource_panel: None,
            network_policy_resource_panel: None,
            node_resource_panel: None,
            persistent_volume_claim_resource_panel: None,
            persistent_volume_resource_panel: None,
            pod_disruption_budget_resource_panel: None,
            pod_resource_panel: None,
            priority_class_resource_panel: None,
            replica_set_resource_panel: None,
            resource_quota_resource_panel: None,
            role_binding_resource_panel: None,
            role_resource_panel: None,
            runtime_class_resource_panel: None,
            secret_resource_panel: None,
            service_account_resource_panel: None,
            service_resource_panel: None,
            storage_class_resource_panel: None,
            stateful_set_resource_panel: None,
            validating_webhook_configuration_resource_panel: None,
            custom_resources_panel: None,
            agent_panels: None,
            agent_turn_requests: Vec::new(),
            agent_conversation_requests: Vec::new(),
            status_load_requests: Vec::new(),
            resource_load_requests: Vec::new(),
            resource_watch_requests: Vec::new(),
            resource_action_requests: Vec::new(),
            pod_log_requests: Vec::new(),
            pod_attach_requests: Vec::new(),
            pod_attach_input_requests: Vec::new(),
        }
    }

    fn status_report() -> ClusterStatusReport {
        ClusterStatusReport {
            overview: miku_api::ClusterStatusOverview {
                version: "v1.35.0".to_owned(),
                platform: Some("darwin/arm64".to_owned()),
                namespaces: 1,
                nodes: 1,
                pods: 1,
                ready_nodes: 1,
                unhealthy_pods: 0,
            },
            conditions: Vec::new(),
            workloads: ClusterStatusWorkloadSummary {
                pods: 1,
                deployments: 1,
                services: 1,
                config_maps: 1,
                secrets: 1,
            },
            recent_events: Vec::new(),
        }
    }
}
