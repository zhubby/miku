use eframe::egui;
use miku_core::ClusterId;

use super::storage_shared::{StorageResourceKind, StorageResourcePanel};
use super::{ResourcePanelRequests, ResourceUiEvent};

#[derive(Clone, Debug)]
pub(crate) struct PersistentVolumeResourcePanel {
    inner: StorageResourcePanel,
}

impl Default for PersistentVolumeResourcePanel {
    fn default() -> Self {
        Self {
            inner: StorageResourcePanel::new(StorageResourceKind::PersistentVolume),
        }
    }
}

impl PersistentVolumeResourcePanel {
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
