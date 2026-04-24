use std::sync::Arc;

use rmcp::handler::server::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, Content, ServerCapabilities, ServerInfo};
use rmcp::{ErrorData as McpError, ServerHandler};
use rmcp::{tool, tool_handler, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::contact_store::{ContactStoreHandle, ContactsCreateParams, ContactsUpdateParams};

// Input length caps. We reject oversized strings at the MCP boundary rather than
// letting them reach the Contacts framework, where limits and failure modes vary
// by macOS version.
const MAX_NAME_LEN: usize = 256;
const MAX_EMAIL_LEN: usize = 320; // RFC 5321
const MAX_PHONE_LEN: usize = 64;
const MAX_ORG_LEN: usize = 256;
const MAX_JOB_LEN: usize = 256;
const MAX_NOTE_LEN: usize = 4096;
const MAX_ID_LEN: usize = 256;
const MAX_QUERY_LEN: usize = 256;

fn check_len(field: &str, value: &str, max: usize) -> Result<(), McpError> {
    if value.len() > max {
        return Err(McpError::invalid_params(
            format!("{field} too long ({} bytes, max {max})", value.len()),
            None,
        ));
    }
    Ok(())
}

fn check_opt_len(field: &str, value: Option<&String>, max: usize) -> Result<(), McpError> {
    if let Some(v) = value {
        check_len(field, v, max)?;
    }
    Ok(())
}

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
        check_len("query", &params.query, MAX_QUERY_LEN)?;
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
        check_len("id", &params.id, MAX_ID_LEN)?;
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

        check_opt_len("first_name", params.first_name.as_ref(), MAX_NAME_LEN)?;
        check_opt_len("last_name", params.last_name.as_ref(), MAX_NAME_LEN)?;
        check_opt_len("email", params.email.as_ref(), MAX_EMAIL_LEN)?;
        check_opt_len("phone", params.phone.as_ref(), MAX_PHONE_LEN)?;
        check_opt_len("organization", params.organization.as_ref(), MAX_ORG_LEN)?;
        check_opt_len("job_title", params.job_title.as_ref(), MAX_JOB_LEN)?;
        check_opt_len("note", params.note.as_ref(), MAX_NOTE_LEN)?;

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
        check_len("id", &params.id, MAX_ID_LEN)?;
        check_opt_len("first_name", params.first_name.as_ref(), MAX_NAME_LEN)?;
        check_opt_len("last_name", params.last_name.as_ref(), MAX_NAME_LEN)?;
        check_opt_len("organization", params.organization.as_ref(), MAX_ORG_LEN)?;
        check_opt_len("job_title", params.job_title.as_ref(), MAX_JOB_LEN)?;
        check_opt_len("note", params.note.as_ref(), MAX_NOTE_LEN)?;

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
        check_len("id", &params.id, MAX_ID_LEN)?;
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
        check_len("name", &params.name, MAX_NAME_LEN)?;
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

#[tool_handler(router = self.tool_router)]
impl ServerHandler for ContactsServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build()).with_instructions(
            "macOS Contacts MCP server. Provides tools to list, search, create, update, and delete contacts and groups from the macOS Contacts app.",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_len_accepts_value_under_limit() {
        assert!(check_len("field", "hello", 10).is_ok());
    }

    #[test]
    fn check_len_accepts_value_at_limit() {
        assert!(check_len("field", "hello", 5).is_ok());
    }

    #[test]
    fn check_len_rejects_value_over_limit() {
        let err = check_len("field", "hello!", 5).unwrap_err();
        assert!(err.to_string().contains("field"));
        assert!(err.to_string().contains("too long"));
    }

    #[test]
    fn check_opt_len_accepts_none() {
        assert!(check_opt_len("field", None, 5).is_ok());
    }

    #[test]
    fn check_opt_len_rejects_oversized_some() {
        let v = "x".repeat(20);
        assert!(check_opt_len("field", Some(&v), 5).is_err());
    }

    #[test]
    fn contacts_list_input_deserializes_with_limit() {
        let input: ContactsListInput = serde_json::from_str(r#"{"limit": 42}"#).unwrap();
        assert_eq!(input.limit, Some(42));
    }

    #[test]
    fn contacts_list_input_deserializes_without_limit() {
        let input: ContactsListInput = serde_json::from_str(r#"{}"#).unwrap();
        assert_eq!(input.limit, None);
    }

    #[test]
    fn contacts_create_input_requires_no_fields_at_deserialize_time() {
        // Validation of "at least one of" happens in the handler, not serde.
        let input: ContactsCreateInput = serde_json::from_str(r#"{}"#).unwrap();
        assert!(input.first_name.is_none());
        assert!(input.last_name.is_none());
        assert!(input.organization.is_none());
    }

    #[test]
    fn contacts_create_input_accepts_full_payload() {
        let json = r#"{
            "first_name": "Ada",
            "last_name": "Lovelace",
            "email": "ada@example.org",
            "phone": "+1-555-0100",
            "organization": "Analytical Engine Co.",
            "job_title": "Mathematician",
            "note": "inventor of the algorithm"
        }"#;
        let input: ContactsCreateInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.first_name.as_deref(), Some("Ada"));
        assert_eq!(input.email.as_deref(), Some("ada@example.org"));
    }

    // Trip-wire: catches accidental downgrades of the limit constants.
    // A compile-time check is stricter and cheaper than a runtime test.
    const _: () = {
        assert!(MAX_EMAIL_LEN >= 320);
        assert!(MAX_NAME_LEN >= 64);
        assert!(MAX_NOTE_LEN >= 1024);
    };
}
