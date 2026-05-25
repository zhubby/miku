use async_trait::async_trait;
use miku_api::{
    AgentContext, AgentConversation, AgentConversationStore, AgentConversationSummary,
    AgentPersistedMessage, AgentRole, AppendAgentMessageRequest, CreateAgentConversationRequest,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, IntoActiveModel, QueryFilter, QueryOrder, Set,
};

use crate::agent_conversations;
use crate::agent_messages;
use crate::store::SqliteStore;
use crate::util::{storage_id, to_storage_error, unix_timestamp};

#[async_trait]
impl AgentConversationStore for SqliteStore {
    #[tracing::instrument(name = "agent_conversations.list", skip(self))]
    async fn list_agent_conversations(&self) -> miku_core::Result<Vec<AgentConversationSummary>> {
        let conversations = agent_conversations::Entity::find()
            .order_by_desc(agent_conversations::Column::LastMessageAt)
            .order_by_desc(agent_conversations::Column::UpdatedAt)
            .all(&self.database)
            .await
            .map_err(to_storage_error)?;

        conversations.into_iter().map(summary_from_model).collect()
    }

    #[tracing::instrument(name = "agent_conversations.get", skip(self), fields(conversation_id = %id))]
    async fn get_agent_conversation(
        &self,
        id: &str,
    ) -> miku_core::Result<Option<AgentConversation>> {
        let Some(conversation) = agent_conversations::Entity::find_by_id(id.to_owned())
            .one(&self.database)
            .await
            .map_err(to_storage_error)?
        else {
            return Ok(None);
        };

        let messages = agent_messages::Entity::find()
            .filter(agent_messages::Column::ConversationId.eq(id))
            .order_by_asc(agent_messages::Column::CreatedAt)
            .all(&self.database)
            .await
            .map_err(to_storage_error)?
            .into_iter()
            .map(message_from_model)
            .collect::<miku_core::Result<Vec<_>>>()?;

        Ok(Some(AgentConversation {
            summary: summary_from_model(conversation)?,
            messages,
        }))
    }

    #[tracing::instrument(name = "agent_conversations.create", skip(self, request))]
    async fn create_agent_conversation(
        &self,
        request: CreateAgentConversationRequest,
    ) -> miku_core::Result<AgentConversationSummary> {
        let timestamp = unix_timestamp();
        let title = request
            .title
            .unwrap_or_else(|| "New conversation".to_owned());
        let context = serde_json::to_string(&request.context).map_err(to_storage_error)?;
        let model = agent_conversations::ActiveModel {
            id: Set(storage_id("agent-conversation")),
            title: Set(title),
            context: Set(context),
            created_at: Set(timestamp),
            updated_at: Set(timestamp),
            last_message_at: Set(None),
        }
        .insert(&self.database)
        .await
        .map_err(to_storage_error)?;

        summary_from_model(model)
    }

    #[tracing::instrument(name = "agent_conversations.append_message", skip(self, request), fields(conversation_id = %request.conversation_id))]
    async fn append_agent_message(
        &self,
        request: AppendAgentMessageRequest,
    ) -> miku_core::Result<AgentPersistedMessage> {
        let Some(conversation) =
            agent_conversations::Entity::find_by_id(request.conversation_id.clone())
                .one(&self.database)
                .await
                .map_err(to_storage_error)?
        else {
            return Err(miku_core::MikuError::NotFound(format!(
                "agent conversation '{}' was not found",
                request.conversation_id
            )));
        };

        let timestamp = unix_timestamp();
        let message = agent_messages::ActiveModel {
            id: Set(storage_id("agent-message")),
            conversation_id: Set(request.conversation_id),
            role: Set(role_to_storage(&request.role).to_owned()),
            content: Set(request.content),
            created_at: Set(timestamp),
        }
        .insert(&self.database)
        .await
        .map_err(to_storage_error)?;

        let mut active_conversation = conversation.into_active_model();
        active_conversation.updated_at = Set(timestamp);
        active_conversation.last_message_at = Set(Some(timestamp));
        active_conversation
            .update(&self.database)
            .await
            .map_err(to_storage_error)?;

        message_from_model(message)
    }

    #[tracing::instrument(name = "agent_conversations.delete", skip(self), fields(conversation_id = %id))]
    async fn delete_agent_conversation(&self, id: &str) -> miku_core::Result<()> {
        agent_messages::Entity::delete_many()
            .filter(agent_messages::Column::ConversationId.eq(id))
            .exec(&self.database)
            .await
            .map_err(to_storage_error)?;
        agent_conversations::Entity::delete_by_id(id.to_owned())
            .exec(&self.database)
            .await
            .map_err(to_storage_error)?;
        Ok(())
    }
}

fn summary_from_model(
    model: agent_conversations::Model,
) -> miku_core::Result<AgentConversationSummary> {
    Ok(AgentConversationSummary {
        id: model.id,
        title: model.title,
        context: serde_json::from_str::<AgentContext>(&model.context).map_err(to_storage_error)?,
        created_at: model.created_at,
        updated_at: model.updated_at,
        last_message_at: model.last_message_at,
    })
}

fn message_from_model(model: agent_messages::Model) -> miku_core::Result<AgentPersistedMessage> {
    Ok(AgentPersistedMessage {
        id: model.id,
        conversation_id: model.conversation_id,
        role: role_from_storage(&model.role)?,
        content: model.content,
        created_at: model.created_at,
    })
}

fn role_to_storage(role: &AgentRole) -> &'static str {
    match role {
        AgentRole::User => "user",
        AgentRole::Assistant => "assistant",
        AgentRole::Tool => "tool",
    }
}

fn role_from_storage(value: &str) -> miku_core::Result<AgentRole> {
    match value {
        "user" => Ok(AgentRole::User),
        "assistant" => Ok(AgentRole::Assistant),
        "tool" => Ok(AgentRole::Tool),
        _ => Err(miku_core::MikuError::Storage(format!(
            "invalid agent message role '{value}'"
        ))),
    }
}
