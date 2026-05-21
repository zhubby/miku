use eframe::egui;
use miku_core::ClusterId;

use super::config_shared::{ConfigResourceKind, ConfigResourcePanel};
use super::{ResourcePanelRequests, ResourceUiEvent};

#[derive(Clone, Debug)]
pub(crate) struct PriorityClassResourcePanel {
    inner: ConfigResourcePanel,
}

impl Default for PriorityClassResourcePanel {
    fn default() -> Self {
        Self {
            inner: ConfigResourcePanel::new(ConfigResourceKind::PriorityClass),
        }
    }
}

impl PriorityClassResourcePanel {
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
