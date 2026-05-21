use eframe::egui;
use miku_core::ClusterId;

use super::access_control_shared::{AccessControlResourceKind, AccessControlResourcePanel};
use super::{ResourcePanelRequests, ResourceUiEvent};

#[derive(Clone, Debug)]
pub(crate) struct RoleResourcePanel {
    inner: AccessControlResourcePanel,
}

impl Default for RoleResourcePanel {
    fn default() -> Self {
        Self {
            inner: AccessControlResourcePanel::new(AccessControlResourceKind::Role),
        }
    }
}

impl RoleResourcePanel {
    pub(crate) fn show(
        &mut self,
        ui: &mut egui::Ui,
        cluster_id: Option<&ClusterId>,
    ) -> ResourcePanelRequests {
        self.inner.show(ui, cluster_id)
    }

    pub(crate) fn apply_event(&mut self, event: ResourceUiEvent) {
        self.inner.apply_event(event);
    }
}
