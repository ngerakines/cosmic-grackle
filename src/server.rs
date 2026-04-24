use std::sync::Arc;

use rmcp::handler::server::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, Content, ServerCapabilities, ServerInfo};
use rmcp::{ErrorData as McpError, ServerHandler};
use rmcp::{tool, tool_handler, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::contact_store::{
    ContactStoreHandle, ContactsCreateParams, ContactsUpdateParams, EmailOp, PhoneOp,
    PostalAddressData, PostalOp, UrlOp,
};

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
const MAX_URL_LEN: usize = 2048;
const MAX_POSTAL_FIELD_LEN: usize = 512;
const MAX_BIRTHDAY_LEN: usize = 32;
const MAX_TYPE_LEN: usize = 32;

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

fn reject_empty(field: &str, value: Option<&String>) -> Result<(), McpError> {
    if let Some(v) = value
        && v.is_empty()
    {
        return Err(McpError::invalid_params(
            format!("{field} must not be empty"),
            None,
        ));
    }
    Ok(())
}

fn fold_phone_op(from: Option<String>, to: Option<String>) -> Option<PhoneOp> {
    match (from, to) {
        (None, None) => None,
        (None, Some(to)) => Some(PhoneOp::Add(to)),
        (Some(from), None) => Some(PhoneOp::Remove(from)),
        (Some(from), Some(to)) => Some(PhoneOp::Replace { from, to }),
    }
}

fn fold_email_op(from: Option<String>, to: Option<String>) -> Option<EmailOp> {
    match (from, to) {
        (None, None) => None,
        (None, Some(to)) => Some(EmailOp::Add(to)),
        (Some(from), None) => Some(EmailOp::Remove(from)),
        (Some(from), Some(to)) => Some(EmailOp::Replace { from, to }),
    }
}

fn fold_url_op(from: Option<String>, to: Option<String>) -> Option<UrlOp> {
    match (from, to) {
        (None, None) => None,
        (None, Some(to)) => Some(UrlOp::Add(to)),
        (Some(from), None) => Some(UrlOp::Remove(from)),
        (Some(from), Some(to)) => Some(UrlOp::Replace { from, to }),
    }
}

fn fold_postal_op(from: Option<String>, to: Option<PostalAddressInput>) -> Option<PostalOp> {
    match (from, to) {
        (None, None) => None,
        (None, Some(to)) => Some(PostalOp::Add(to.into())),
        (Some(from), None) => Some(PostalOp::Remove(from)),
        (Some(from), Some(to)) => Some(PostalOp::Replace {
            from,
            to: to.into(),
        }),
    }
}

#[derive(Debug, Deserialize, JsonSchema, Clone)]
pub struct PostalAddressInput {
    /// Street address (e.g., "123 Main St").
    pub street: Option<String>,
    /// City or locality.
    pub city: Option<String>,
    /// State, province, or region.
    pub state: Option<String>,
    /// ZIP or postal code.
    pub postal_code: Option<String>,
    /// Country name.
    pub country: Option<String>,
}

impl From<PostalAddressInput> for PostalAddressData {
    fn from(v: PostalAddressInput) -> Self {
        Self {
            street: v.street,
            city: v.city,
            state: v.state,
            postal_code: v.postal_code,
            country: v.country,
        }
    }
}

fn check_postal_input(field: &str, value: Option<&PostalAddressInput>) -> Result<(), McpError> {
    let Some(v) = value else { return Ok(()) };
    check_opt_len(
        &format!("{field}.street"),
        v.street.as_ref(),
        MAX_POSTAL_FIELD_LEN,
    )?;
    check_opt_len(
        &format!("{field}.city"),
        v.city.as_ref(),
        MAX_POSTAL_FIELD_LEN,
    )?;
    check_opt_len(
        &format!("{field}.state"),
        v.state.as_ref(),
        MAX_POSTAL_FIELD_LEN,
    )?;
    check_opt_len(
        &format!("{field}.postal_code"),
        v.postal_code.as_ref(),
        MAX_POSTAL_FIELD_LEN,
    )?;
    check_opt_len(
        &format!("{field}.country"),
        v.country.as_ref(),
        MAX_POSTAL_FIELD_LEN,
    )?;
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
    /// Contact type: "person" (default) or "organization".
    pub contact_type: Option<String>,
    /// Honorific prefix, e.g. "Dr.", "Ms.".
    pub name_prefix: Option<String>,
    /// First name.
    pub first_name: Option<String>,
    /// Middle name.
    pub middle_name: Option<String>,
    /// Last name.
    pub last_name: Option<String>,
    /// Name suffix, e.g. "Jr.", "III", "PhD".
    pub name_suffix: Option<String>,
    /// Nickname.
    pub nickname: Option<String>,
    /// Phonetic given name (for kana / CJK sort).
    pub phonetic_given_name: Option<String>,
    /// Phonetic middle name.
    pub phonetic_middle_name: Option<String>,
    /// Phonetic family name.
    pub phonetic_family_name: Option<String>,
    /// Phonetic organization name.
    pub phonetic_organization_name: Option<String>,
    /// Email address (stored with label "work").
    pub email: Option<String>,
    /// Phone number (stored with label "mobile").
    pub phone: Option<String>,
    /// Website URL (stored with label "homepage").
    pub url: Option<String>,
    /// Organization or company name.
    pub organization: Option<String>,
    /// Department within the organization.
    pub department: Option<String>,
    /// Job title.
    pub job_title: Option<String>,
    /// A note about the contact.
    pub note: Option<String>,
    /// Birthday as "YYYY-MM-DD" or "--MM-DD" (month/day without year).
    pub birthday: Option<String>,
    /// Postal address (stored with label "home"). Any subset of the five
    /// sub-fields may be provided; unspecified sub-fields remain empty.
    pub postal_address: Option<PostalAddressInput>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ContactsUpdateInput {
    /// The unique contact identifier.
    pub id: String,
    /// New contact type: "person" or "organization".
    pub contact_type: Option<String>,
    /// New honorific prefix.
    pub name_prefix: Option<String>,
    /// New first name.
    pub first_name: Option<String>,
    /// New middle name.
    pub middle_name: Option<String>,
    /// New last name.
    pub last_name: Option<String>,
    /// New name suffix.
    pub name_suffix: Option<String>,
    /// New nickname.
    pub nickname: Option<String>,
    /// New phonetic given name.
    pub phonetic_given_name: Option<String>,
    /// New phonetic middle name.
    pub phonetic_middle_name: Option<String>,
    /// New phonetic family name.
    pub phonetic_family_name: Option<String>,
    /// New phonetic organization name.
    pub phonetic_organization_name: Option<String>,
    /// New organization name.
    pub organization: Option<String>,
    /// New department name.
    pub department: Option<String>,
    /// New job title.
    pub job_title: Option<String>,
    /// New note.
    pub note: Option<String>,
    /// New birthday as "YYYY-MM-DD" or "--MM-DD".
    pub birthday: Option<String>,
    /// Existing phone number to find on the contact. Exact match against the
    /// stored form — call contacts_get first to see it. Pair with phone_to:
    /// omit phone_from to add phone_to as a new entry; omit phone_to to
    /// remove phone_from; set both to replace phone_from with phone_to
    /// while preserving the original label.
    pub phone_from: Option<String>,
    /// New phone number. See phone_from for the pairing rules.
    pub phone_to: Option<String>,
    /// Existing email to find. Exact match against the stored value. Pair
    /// with email_to using the same omit-one semantics as phone_from/phone_to.
    pub email_from: Option<String>,
    /// New email. See email_from for the pairing rules.
    pub email_to: Option<String>,
    /// Existing URL to find. Exact match. Pair with url_to using the same
    /// omit-one semantics as phone_from/phone_to.
    pub url_from: Option<String>,
    /// New URL. See url_from for the pairing rules.
    pub url_to: Option<String>,
    /// Existing postal address to find, matched by the joined-string form
    /// shown in `Contact.addresses` (e.g., "123 Main St, Austin, TX, 78701, USA").
    /// Pair with postal_to using the same omit-one semantics as phone_from/phone_to.
    pub postal_from: Option<String>,
    /// New postal address. See postal_from for the pairing rules.
    pub postal_to: Option<PostalAddressInput>,
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
        description = "Create a new contact in macOS Contacts. At least one of first_name, last_name, or organization is required. Scalar fields (names, nickname, prefix/suffix, department, job_title, note, contact_type, birthday) can be set directly. Exactly one phone, email, url, and postal address may be set via the shorthand fields — each gets a default label (phone=mobile, email=work, url=homepage, postal_address=home). For multiple entries or custom labels, create first then use contacts_update. Returns the new contact's identifier."
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

        check_opt_len("contact_type", params.contact_type.as_ref(), MAX_TYPE_LEN)?;
        check_opt_len("name_prefix", params.name_prefix.as_ref(), MAX_NAME_LEN)?;
        check_opt_len("first_name", params.first_name.as_ref(), MAX_NAME_LEN)?;
        check_opt_len("middle_name", params.middle_name.as_ref(), MAX_NAME_LEN)?;
        check_opt_len("last_name", params.last_name.as_ref(), MAX_NAME_LEN)?;
        check_opt_len("name_suffix", params.name_suffix.as_ref(), MAX_NAME_LEN)?;
        check_opt_len("nickname", params.nickname.as_ref(), MAX_NAME_LEN)?;
        check_opt_len(
            "phonetic_given_name",
            params.phonetic_given_name.as_ref(),
            MAX_NAME_LEN,
        )?;
        check_opt_len(
            "phonetic_middle_name",
            params.phonetic_middle_name.as_ref(),
            MAX_NAME_LEN,
        )?;
        check_opt_len(
            "phonetic_family_name",
            params.phonetic_family_name.as_ref(),
            MAX_NAME_LEN,
        )?;
        check_opt_len(
            "phonetic_organization_name",
            params.phonetic_organization_name.as_ref(),
            MAX_NAME_LEN,
        )?;
        check_opt_len("email", params.email.as_ref(), MAX_EMAIL_LEN)?;
        check_opt_len("phone", params.phone.as_ref(), MAX_PHONE_LEN)?;
        check_opt_len("url", params.url.as_ref(), MAX_URL_LEN)?;
        check_opt_len("organization", params.organization.as_ref(), MAX_ORG_LEN)?;
        check_opt_len("department", params.department.as_ref(), MAX_ORG_LEN)?;
        check_opt_len("job_title", params.job_title.as_ref(), MAX_JOB_LEN)?;
        check_opt_len("note", params.note.as_ref(), MAX_NOTE_LEN)?;
        check_opt_len("birthday", params.birthday.as_ref(), MAX_BIRTHDAY_LEN)?;
        check_postal_input("postal_address", params.postal_address.as_ref())?;

        let store_params = ContactsCreateParams {
            contact_type: params.contact_type,
            name_prefix: params.name_prefix,
            first_name: params.first_name,
            middle_name: params.middle_name,
            last_name: params.last_name,
            name_suffix: params.name_suffix,
            nickname: params.nickname,
            phonetic_given_name: params.phonetic_given_name,
            phonetic_middle_name: params.phonetic_middle_name,
            phonetic_family_name: params.phonetic_family_name,
            phonetic_organization_name: params.phonetic_organization_name,
            email: params.email,
            phone: params.phone,
            url: params.url,
            organization: params.organization,
            department: params.department,
            job_title: params.job_title,
            note: params.note,
            birthday: params.birthday,
            postal_address: params.postal_address.map(Into::into),
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
        description = "Update an existing contact. Requires the contact's identifier. Only provided fields are updated. Phones, emails, URLs, and postal addresses are edited via paired fields (e.g. phone_from/phone_to): set only _to to add a new entry, set only _from to remove an entry, set both to replace _from with _to (the original label is preserved). Match is exact — call contacts_get first to see the stored form. Postal addresses match on the joined-string form exposed in Contact.addresses."
    )]
    async fn contacts_update(
        &self,
        Parameters(params): Parameters<ContactsUpdateInput>,
    ) -> Result<CallToolResult, McpError> {
        check_len("id", &params.id, MAX_ID_LEN)?;
        check_opt_len("contact_type", params.contact_type.as_ref(), MAX_TYPE_LEN)?;
        check_opt_len("name_prefix", params.name_prefix.as_ref(), MAX_NAME_LEN)?;
        check_opt_len("first_name", params.first_name.as_ref(), MAX_NAME_LEN)?;
        check_opt_len("middle_name", params.middle_name.as_ref(), MAX_NAME_LEN)?;
        check_opt_len("last_name", params.last_name.as_ref(), MAX_NAME_LEN)?;
        check_opt_len("name_suffix", params.name_suffix.as_ref(), MAX_NAME_LEN)?;
        check_opt_len("nickname", params.nickname.as_ref(), MAX_NAME_LEN)?;
        check_opt_len(
            "phonetic_given_name",
            params.phonetic_given_name.as_ref(),
            MAX_NAME_LEN,
        )?;
        check_opt_len(
            "phonetic_middle_name",
            params.phonetic_middle_name.as_ref(),
            MAX_NAME_LEN,
        )?;
        check_opt_len(
            "phonetic_family_name",
            params.phonetic_family_name.as_ref(),
            MAX_NAME_LEN,
        )?;
        check_opt_len(
            "phonetic_organization_name",
            params.phonetic_organization_name.as_ref(),
            MAX_NAME_LEN,
        )?;
        check_opt_len("organization", params.organization.as_ref(), MAX_ORG_LEN)?;
        check_opt_len("department", params.department.as_ref(), MAX_ORG_LEN)?;
        check_opt_len("job_title", params.job_title.as_ref(), MAX_JOB_LEN)?;
        check_opt_len("note", params.note.as_ref(), MAX_NOTE_LEN)?;
        check_opt_len("birthday", params.birthday.as_ref(), MAX_BIRTHDAY_LEN)?;
        check_opt_len("phone_from", params.phone_from.as_ref(), MAX_PHONE_LEN)?;
        check_opt_len("phone_to", params.phone_to.as_ref(), MAX_PHONE_LEN)?;
        check_opt_len("email_from", params.email_from.as_ref(), MAX_EMAIL_LEN)?;
        check_opt_len("email_to", params.email_to.as_ref(), MAX_EMAIL_LEN)?;
        check_opt_len("url_from", params.url_from.as_ref(), MAX_URL_LEN)?;
        check_opt_len("url_to", params.url_to.as_ref(), MAX_URL_LEN)?;
        check_opt_len(
            "postal_from",
            params.postal_from.as_ref(),
            MAX_POSTAL_FIELD_LEN * 6,
        )?;
        check_postal_input("postal_to", params.postal_to.as_ref())?;
        reject_empty("phone_from", params.phone_from.as_ref())?;
        reject_empty("phone_to", params.phone_to.as_ref())?;
        reject_empty("email_from", params.email_from.as_ref())?;
        reject_empty("email_to", params.email_to.as_ref())?;
        reject_empty("url_from", params.url_from.as_ref())?;
        reject_empty("url_to", params.url_to.as_ref())?;
        reject_empty("postal_from", params.postal_from.as_ref())?;

        let phone_op = fold_phone_op(params.phone_from, params.phone_to);
        let email_op = fold_email_op(params.email_from, params.email_to);
        let url_op = fold_url_op(params.url_from, params.url_to);
        let postal_op = fold_postal_op(params.postal_from, params.postal_to);

        let store_params = ContactsUpdateParams {
            id: params.id,
            contact_type: params.contact_type,
            name_prefix: params.name_prefix,
            first_name: params.first_name,
            middle_name: params.middle_name,
            last_name: params.last_name,
            name_suffix: params.name_suffix,
            nickname: params.nickname,
            phonetic_given_name: params.phonetic_given_name,
            phonetic_middle_name: params.phonetic_middle_name,
            phonetic_family_name: params.phonetic_family_name,
            phonetic_organization_name: params.phonetic_organization_name,
            organization: params.organization,
            department: params.department,
            job_title: params.job_title,
            note: params.note,
            birthday: params.birthday,
            phone_op,
            email_op,
            url_op,
            postal_op,
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

    #[test]
    fn reject_empty_accepts_none() {
        assert!(reject_empty("field", None).is_ok());
    }

    #[test]
    fn reject_empty_accepts_non_empty() {
        let v = "x".to_string();
        assert!(reject_empty("field", Some(&v)).is_ok());
    }

    #[test]
    fn reject_empty_rejects_empty_string() {
        let v = String::new();
        let err = reject_empty("phone_from", Some(&v)).unwrap_err();
        assert!(err.to_string().contains("phone_from"));
        assert!(err.to_string().contains("must not be empty"));
    }

    #[test]
    fn fold_phone_op_none_when_both_absent() {
        assert!(fold_phone_op(None, None).is_none());
    }

    #[test]
    fn fold_phone_op_add_when_only_to_set() {
        match fold_phone_op(None, Some("555-0100".into())) {
            Some(PhoneOp::Add(v)) => assert_eq!(v, "555-0100"),
            other => panic!("expected Add, got {other:?}"),
        }
    }

    #[test]
    fn fold_phone_op_remove_when_only_from_set() {
        match fold_phone_op(Some("555-0100".into()), None) {
            Some(PhoneOp::Remove(v)) => assert_eq!(v, "555-0100"),
            other => panic!("expected Remove, got {other:?}"),
        }
    }

    #[test]
    fn fold_phone_op_replace_when_both_set() {
        match fold_phone_op(Some("old".into()), Some("new".into())) {
            Some(PhoneOp::Replace { from, to }) => {
                assert_eq!(from, "old");
                assert_eq!(to, "new");
            }
            other => panic!("expected Replace, got {other:?}"),
        }
    }

    #[test]
    fn fold_email_op_covers_all_four_combinations() {
        assert!(fold_email_op(None, None).is_none());
        assert!(matches!(
            fold_email_op(None, Some("a@b".into())),
            Some(EmailOp::Add(_))
        ));
        assert!(matches!(
            fold_email_op(Some("a@b".into()), None),
            Some(EmailOp::Remove(_))
        ));
        assert!(matches!(
            fold_email_op(Some("a@b".into()), Some("c@d".into())),
            Some(EmailOp::Replace { .. })
        ));
    }

    #[test]
    fn contacts_update_input_deserializes_with_phone_and_email_pairs() {
        let json = r#"{
            "id": "ABC-123",
            "phone_from": "555-0100",
            "phone_to": "555-0200",
            "email_from": "old@example.org",
            "email_to": "new@example.org"
        }"#;
        let input: ContactsUpdateInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.phone_from.as_deref(), Some("555-0100"));
        assert_eq!(input.phone_to.as_deref(), Some("555-0200"));
        assert_eq!(input.email_from.as_deref(), Some("old@example.org"));
        assert_eq!(input.email_to.as_deref(), Some("new@example.org"));
    }

    #[test]
    fn contacts_update_input_defaults_phone_email_fields_to_none() {
        let input: ContactsUpdateInput = serde_json::from_str(r#"{"id": "X"}"#).unwrap();
        assert!(input.phone_from.is_none());
        assert!(input.phone_to.is_none());
        assert!(input.email_from.is_none());
        assert!(input.email_to.is_none());
    }

    #[test]
    fn check_opt_len_rejects_oversized_phone_from() {
        let v = "x".repeat(MAX_PHONE_LEN + 1);
        assert!(check_opt_len("phone_from", Some(&v), MAX_PHONE_LEN).is_err());
    }

    #[test]
    fn check_opt_len_rejects_oversized_email_to() {
        let v = "x".repeat(MAX_EMAIL_LEN + 1);
        assert!(check_opt_len("email_to", Some(&v), MAX_EMAIL_LEN).is_err());
    }

    #[test]
    fn fold_url_op_covers_all_four_combinations() {
        assert!(fold_url_op(None, None).is_none());
        assert!(matches!(
            fold_url_op(None, Some("https://a".into())),
            Some(UrlOp::Add(_))
        ));
        assert!(matches!(
            fold_url_op(Some("https://a".into()), None),
            Some(UrlOp::Remove(_))
        ));
        assert!(matches!(
            fold_url_op(Some("https://a".into()), Some("https://b".into())),
            Some(UrlOp::Replace { .. })
        ));
    }

    #[test]
    fn fold_postal_op_covers_all_four_combinations() {
        let addr = || PostalAddressInput {
            street: Some("123 Main".into()),
            city: Some("Austin".into()),
            state: None,
            postal_code: None,
            country: None,
        };
        assert!(fold_postal_op(None, None).is_none());
        assert!(matches!(
            fold_postal_op(None, Some(addr())),
            Some(PostalOp::Add(_))
        ));
        assert!(matches!(
            fold_postal_op(Some("old".into()), None),
            Some(PostalOp::Remove(_))
        ));
        assert!(matches!(
            fold_postal_op(Some("old".into()), Some(addr())),
            Some(PostalOp::Replace { .. })
        ));
    }

    #[test]
    fn check_postal_input_accepts_none() {
        assert!(check_postal_input("postal_to", None).is_ok());
    }

    #[test]
    fn check_postal_input_accepts_empty_fields() {
        // Every subfield is None — validation only caps lengths, doesn't require
        // at least one field (that's enforced at the store level for add/replace).
        let empty = PostalAddressInput {
            street: None,
            city: None,
            state: None,
            postal_code: None,
            country: None,
        };
        assert!(check_postal_input("postal_to", Some(&empty)).is_ok());
    }

    #[test]
    fn check_postal_input_rejects_oversized_street() {
        let huge = "x".repeat(MAX_POSTAL_FIELD_LEN + 1);
        let addr = PostalAddressInput {
            street: Some(huge),
            city: None,
            state: None,
            postal_code: None,
            country: None,
        };
        let err = check_postal_input("postal_to", Some(&addr)).unwrap_err();
        assert!(err.to_string().contains("postal_to.street"));
    }

    #[test]
    fn contacts_create_input_accepts_all_new_fields() {
        let json = r#"{
            "contact_type": "person",
            "name_prefix": "Dr.",
            "first_name": "Ada",
            "middle_name": "Augusta",
            "last_name": "Lovelace",
            "name_suffix": "PhD",
            "nickname": "Ada",
            "phonetic_given_name": "Ada",
            "department": "R&D",
            "url": "https://example.org",
            "birthday": "1815-12-10",
            "postal_address": {
                "street": "123 Main St",
                "city": "London",
                "country": "UK"
            }
        }"#;
        let input: ContactsCreateInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.name_prefix.as_deref(), Some("Dr."));
        assert_eq!(input.middle_name.as_deref(), Some("Augusta"));
        assert_eq!(input.name_suffix.as_deref(), Some("PhD"));
        assert_eq!(input.nickname.as_deref(), Some("Ada"));
        assert_eq!(input.department.as_deref(), Some("R&D"));
        assert_eq!(input.url.as_deref(), Some("https://example.org"));
        assert_eq!(input.birthday.as_deref(), Some("1815-12-10"));
        let postal = input.postal_address.unwrap();
        assert_eq!(postal.street.as_deref(), Some("123 Main St"));
        assert_eq!(postal.city.as_deref(), Some("London"));
        assert!(postal.state.is_none());
    }

    #[test]
    fn contacts_update_input_accepts_url_and_postal_pairs() {
        let json = r#"{
            "id": "X",
            "url_from": "https://old.example.org",
            "url_to": "https://new.example.org",
            "postal_from": "123 Main, Austin, TX, 78701",
            "postal_to": { "street": "456 Elm St", "city": "Austin" }
        }"#;
        let input: ContactsUpdateInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.url_from.as_deref(), Some("https://old.example.org"));
        assert_eq!(input.url_to.as_deref(), Some("https://new.example.org"));
        assert_eq!(
            input.postal_from.as_deref(),
            Some("123 Main, Austin, TX, 78701")
        );
        assert_eq!(
            input.postal_to.as_ref().unwrap().street.as_deref(),
            Some("456 Elm St"),
        );
    }
}
