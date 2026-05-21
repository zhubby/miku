use eframe::egui;
use miku_core::ClusterId;

use super::network_shared::{NetworkResourceKind, NetworkResourcePanel};
use super::{ResourcePanelRequests, ResourceUiEvent};

#[derive(Clone, Debug)]
pub(crate) struct EndpointSliceResourcePanel {
    inner: NetworkResourcePanel,
}

impl Default for EndpointSliceResourcePanel {
    fn default() -> Self {
        Self {
            inner: NetworkResourcePanel::new(NetworkResourceKind::EndpointSlice),
        }
    }
}

impl EndpointSliceResourcePanel {
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
