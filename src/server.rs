use std::sync::Arc;

use rmcp::handler::server::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, Content, ServerCapabilities, ServerInfo};
use rmcp::{ErrorData as McpError, ServerHandler};
use rmcp::{tool, tool_handler, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::contact_store::{ContactStoreHandle, ContactsCreateParams, ContactsUpdateParams};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ContactsListInput {
    /// Maximum number of contacts to return. If omitted, returns all contacts.
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ContactsSearchInput {
    /// Search query to match against contact names.
    pub query: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ContactsGetInput {
    /// The unique contact identifier.
    pub id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ContactsCreateInput {
    /// First name of the contact.
    pub first_name: Option<String>,
    /// Last name of the contact.
    pub last_name: Option<String>,
    /// Email address.
    pub email: Option<String>,
    /// Phone number.
    pub phone: Option<String>,
    /// Organization or company name.
    pub organization: Option<String>,
    /// Job title.
    pub job_title: Option<String>,
    /// A note about the contact.
    pub note: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ContactsUpdateInput {
    /// The unique contact identifier.
    pub id: String,
    /// New first name.
    pub first_name: Option<String>,
    /// New last name.
    pub last_name: Option<String>,
    /// New organization name.
    pub organization: Option<String>,
    /// New job title.
    pub job_title: Option<String>,
    /// New note.
    pub note: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ContactsDeleteInput {
    /// The unique contact identifier.
    pub id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GroupsMembersInput {
    /// The group name to look up.
    pub name: String,
}

#[derive(Clone)]
pub struct ContactsServer {
    store: Arc<ContactStoreHandle>,
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl ContactsServer {
    pub fn new(store: ContactStoreHandle) -> Self {
        let tool_router = Self::tool_router();
        Self {
            store: Arc::new(store),
            tool_router,
        }
    }

    #[tool(
        name = "contacts_list",
        description = "List all contacts from macOS Contacts. Returns an array of contact objects with name, email, phone, and other details."
    )]
    async fn contacts_list(
        &self,
        Parameters(params): Parameters<ContactsListInput>,
    ) -> Result<CallToolResult, McpError> {
        let contacts = self
            .store
            .list_contacts(params.limit)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        let json = serde_json::to_string_pretty(&contacts)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(
        name = "contacts_search",
        description = "Search contacts by name. Returns contacts whose name matches the query string."
    )]
    async fn contacts_search(
        &self,
        Parameters(params): Parameters<ContactsSearchInput>,
    ) -> Result<CallToolResult, McpError> {
        let contacts = self
            .store
            .search_contacts(&params.query)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        let json = serde_json::to_string_pretty(&contacts)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(
        name = "contacts_get",
        description = "Get a single contact by their unique identifier. Returns full contact details."
    )]
    async fn contacts_get(
        &self,
        Parameters(params): Parameters<ContactsGetInput>,
    ) -> Result<CallToolResult, McpError> {
        match self
            .store
            .get_contact(&params.id)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?
        {
            Some(contact) => {
                let json = serde_json::to_string_pretty(&contact)
                    .map_err(|e| McpError::internal_error(e.to_string(), None))?;
                Ok(CallToolResult::success(vec![Content::text(json)]))
            }
            None => Ok(CallToolResult::error(vec![Content::text(format!(
                "Contact not found: {}",
                params.id
            ))])),
        }
    }

    #[tool(
        name = "contacts_create",
        description = "Create a new contact in macOS Contacts. At least one of first_name, last_name, or organization is required. Returns the new contact's identifier."
    )]
    async fn contacts_create(
        &self,
        Parameters(params): Parameters<ContactsCreateInput>,
    ) -> Result<CallToolResult, McpError> {
        if params.first_name.is_none()
            && params.last_name.is_none()
            && params.organization.is_none()
        {
            return Err(McpError::invalid_params(
                "At least one of first_name, last_name, or organization is required",
                None,
            ));
        }

        let store_params = ContactsCreateParams {
            first_name: params.first_name,
            last_name: params.last_name,
            email: params.email,
            phone: params.phone,
            organization: params.organization,
            job_title: params.job_title,
            note: params.note,
        };

        let id = self
            .store
            .create_contact(store_params)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        let json = serde_json::json!({"id": id, "success": true});
        Ok(CallToolResult::success(vec![Content::text(
            json.to_string(),
        )]))
    }

    #[tool(
        name = "contacts_update",
        description = "Update an existing contact. Requires the contact's identifier. Only provided fields will be updated."
    )]
    async fn contacts_update(
        &self,
        Parameters(params): Parameters<ContactsUpdateInput>,
    ) -> Result<CallToolResult, McpError> {
        let store_params = ContactsUpdateParams {
            id: params.id,
            first_name: params.first_name,
            last_name: params.last_name,
            organization: params.organization,
            job_title: params.job_title,
            note: params.note,
        };

        let success = self
            .store
            .update_contact(store_params)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        let json = serde_json::json!({"success": success});
        Ok(CallToolResult::success(vec![Content::text(
            json.to_string(),
        )]))
    }

    #[tool(
        name = "contacts_delete",
        description = "Delete a contact by their unique identifier."
    )]
    async fn contacts_delete(
        &self,
        Parameters(params): Parameters<ContactsDeleteInput>,
    ) -> Result<CallToolResult, McpError> {
        let success = self
            .store
            .delete_contact(&params.id)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        let json = serde_json::json!({"success": success});
        Ok(CallToolResult::success(vec![Content::text(
            json.to_string(),
        )]))
    }

    #[tool(
        name = "groups_list",
        description = "List all contact groups with their member counts."
    )]
    async fn groups_list(&self) -> Result<CallToolResult, McpError> {
        let groups = self
            .store
            .list_groups()
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        let json = serde_json::to_string_pretty(&groups)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(
        name = "groups_members",
        description = "Get all contacts that belong to a specific group. The group is looked up by name (case-insensitive)."
    )]
    async fn groups_members(
        &self,
        Parameters(params): Parameters<GroupsMembersInput>,
    ) -> Result<CallToolResult, McpError> {
        let contacts = self
            .store
            .get_group_members(&params.name)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        let json = serde_json::to_string_pretty(&contacts)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }
}

#[tool_handler]
impl ServerHandler for ContactsServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "macOS Contacts MCP server. Provides tools to list, search, create, update, and delete contacts and groups from the macOS Contacts app."
                    .into(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}
