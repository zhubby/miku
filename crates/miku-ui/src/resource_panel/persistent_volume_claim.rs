use eframe::egui;
use miku_core::ClusterId;

use super::storage_shared::{StorageResourceKind, StorageResourcePanel};
use super::{ResourcePanelRequests, ResourceUiEvent};

#[derive(Clone, Debug)]
pub(crate) struct PersistentVolumeClaimResourcePanel {
    inner: StorageResourcePanel,
}

impl Default for PersistentVolumeClaimResourcePanel {
    fn default() -> Self {
        Self {
            inner: StorageResourcePanel::new(StorageResourceKind::PersistentVolumeClaim),
        }
    }
}

impl PersistentVolumeClaimResourcePanel {
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
