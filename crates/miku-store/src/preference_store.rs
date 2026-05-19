use async_trait::async_trait;
use miku_api::LocalPreferenceStore;
use sea_orm::sea_query::OnConflict;
use sea_orm::{EntityTrait, Set};

use crate::preferences;
use crate::store::SqliteStore;
use crate::util::{to_storage_error, unix_timestamp};

#[async_trait]
impl LocalPreferenceStore for SqliteStore {
    #[tracing::instrument(name = "preferences.get", skip(self), fields(key = %key))]
    async fn get_preference(&self, key: &str) -> miku_core::Result<Option<serde_json::Value>> {
        let raw = preferences::Entity::find_by_id(key.to_owned())
            .one(&self.database)
            .await
            .map_err(to_storage_error)?
            .map(|preference| preference.value);
        tracing::debug!(found = raw.is_some(), "loaded preference");

        raw.map(|value| serde_json::from_str(&value).map_err(to_storage_error))
            .transpose()
    }

    #[tracing::instrument(name = "preferences.set", skip(self, value), fields(key = %key))]
    async fn set_preference(&self, key: &str, value: serde_json::Value) -> miku_core::Result<()> {
        let serialized = serde_json::to_string(&value).map_err(to_storage_error)?;
        preferences::Entity::insert(preferences::ActiveModel {
            key: Set(key.to_owned()),
            value: Set(serialized),
            updated_at: Set(unix_timestamp()),
        })
        .on_conflict(
            OnConflict::column(preferences::Column::Key)
                .update_columns([preferences::Column::Value, preferences::Column::UpdatedAt])
                .to_owned(),
        )
        .exec(&self.database)
        .await
        .map_err(to_storage_error)?;

        tracing::debug!("stored preference");
        Ok(())
    }
}
