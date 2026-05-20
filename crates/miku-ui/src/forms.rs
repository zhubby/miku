#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct NewClusterForm {
    pub(crate) open: bool,
    pub(crate) context: String,
    pub(crate) config: String,
    pub(crate) error: Option<String>,
}

impl NewClusterForm {
    pub(crate) fn open(&mut self) {
        self.open = true;
        self.error = None;
    }

    pub(crate) fn cancel(&mut self) {
        *self = Self::default();
    }

    pub(crate) fn save_started(&mut self) {
        self.error = None;
    }

    pub(crate) fn save_failed(&mut self, error: impl Into<String>) {
        self.error = Some(error.into());
    }

    pub(crate) fn save_succeeded(&mut self) {
        self.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_cluster_form_cancel_clears_state() {
        let mut form = NewClusterForm {
            open: true,
            context: "kind-miku".to_owned(),
            config: "apiVersion: v1".to_owned(),
            error: Some("failed".to_owned()),
        };

        form.cancel();

        assert_eq!(form, NewClusterForm::default());
    }

    #[test]
    fn new_cluster_form_save_success_closes_and_clears_state() {
        let mut form = NewClusterForm {
            open: true,
            context: "kind-miku".to_owned(),
            config: "apiVersion: v1".to_owned(),
            error: None,
        };

        form.save_succeeded();

        assert_eq!(form, NewClusterForm::default());
    }

    #[test]
    fn new_cluster_form_save_failure_keeps_input() {
        let mut form = NewClusterForm {
            open: true,
            context: "kind-miku".to_owned(),
            config: "apiVersion: v1".to_owned(),
            error: None,
        };

        form.save_failed("duplicate context");

        assert!(form.open);
        assert_eq!(form.context, "kind-miku");
        assert_eq!(form.config, "apiVersion: v1");
        assert_eq!(form.error.as_deref(), Some("duplicate context"));
    }
}
