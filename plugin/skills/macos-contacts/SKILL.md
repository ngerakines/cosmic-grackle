---
name: macos-contacts
description: Use when the user wants to create, update, or delete a contact in the macOS Contacts app (Apple Contacts) on their local Mac. Triggers on phrases like "add a contact on my Mac", "create a new contact for X", "update Jane's job title", "change Bob's organization", "delete the contact for Alice", "remove a duplicate contact from Contacts.app". Applies only to the native macOS Contacts app backed by the CNContactStore framework — NOT Google Contacts, iCloud.com, CRMs, or address books in third-party apps. Requires the `cosmic-grackle` MCP server (installed separately — see https://github.com/ngerakines/cosmic-grackle) to be registered with the user's MCP client; the server exposes `contacts_search`, `contacts_get`, `contacts_create`, `contacts_update`, `contacts_delete`, `contacts_list`, `groups_list`, and `groups_members`. If those tools are not available in the session, tell the user the MCP server is not installed or registered and link them to the install instructions rather than guessing. The first tool invocation prompts the user to grant Contacts access in System Settings.
---

# macOS Contacts CRUD

This skill covers create / update / delete workflows against the macOS Contacts app using the `cosmic-grackle` MCP server's `contacts_create`, `contacts_update`, and `contacts_delete` tools. Read-only tools (`contacts_list`, `contacts_search`, `contacts_get`, `groups_list`, `groups_members`) are documented here only insofar as they are prerequisites for mutating operations.

The MCP server is distributed independently of this skill and is registered under whatever name the user chose in their MCP client config (typically `contacts`). If the `contacts_*` tools are not present in the current session the server is not installed or not registered — point the user at https://github.com/ngerakines/cosmic-grackle for install instructions instead of attempting the workflow.

## When to use

- The user is on macOS and wants to modify the local Contacts database.
- The request names a specific contact by name or describes one to be created.

## When NOT to use

- The user is managing contacts in Google, iCloud web, Outlook, HubSpot, Salesforce, or any non-macOS store.
- The user wants to rename a label (e.g., change a phone's label from "mobile" to "work") or change group membership — neither custom labels nor group add/remove are exposed by this MCP. Direct them to Contacts.app.

## Identifier model

Every contact has an opaque identifier returned in the `id` field. `contacts_update` and `contacts_delete` require this id — they do NOT accept a name.

Before any update or delete, resolve the id with `contacts_search` (by name substring) or `contacts_list`. If the search returns multiple matches, ask the user which one to act on; do not guess.

`contacts_get` returns the full contact record for a known id and is useful for confirming "am I about to edit the right person?" before a mutation, and for reading the exact stored form of phones / emails / URLs / addresses before using the `_from` ops on update.

## Create

Tool: `contacts_create`

At least one of `first_name`, `last_name`, or `organization` is required. Fields, grouped:

- **Name:** `contact_type` (`"person"` default, or `"organization"`), `name_prefix`, `first_name`, `middle_name`, `last_name`, `name_suffix`, `nickname`, plus phonetic variants `phonetic_given_name`, `phonetic_middle_name`, `phonetic_family_name`, `phonetic_organization_name` (primarily for CJK / kana sort order).
- **Organization:** `organization`, `department`, `job_title`.
- **Contact methods (one entry each, default label in parens):** `email` (work), `phone` (mobile), `url` (homepage), `postal_address` (home). `postal_address` is a structured object with optional `street`, `city`, `state`, `postal_code`, `country` sub-fields — any subset may be provided.
- **Other:** `note`, `birthday` as `"YYYY-MM-DD"` or `"--MM-DD"` (month/day without year).

Only one phone, email, url, and postal address can be set on create. For additional entries or non-default labels, create first and then layer on more with `contacts_update` (each Add call uses the same default label — `Contacts.app` is the only place to rename labels).

Returned JSON: `{"id": "<opaque-id>", "success": true}`. Store the id if subsequent operations are planned.

Example call:

```json
{
  "first_name": "Ada",
  "middle_name": "Augusta",
  "last_name": "Lovelace",
  "organization": "Analytical Engine Co.",
  "job_title": "Mathematician",
  "email": "ada@example.org",
  "url": "https://example.org",
  "birthday": "1815-12-10",
  "postal_address": {
    "street": "12 St. James's Square",
    "city": "London",
    "country": "UK"
  },
  "note": "First programmer"
}
```

Length caps (MCP rejects over-limit input, byte counts): names 256, email 320, phone 64, url 2048, organization 256, department 256, job title 256, note 4096, birthday 32, contact_type 32, each postal sub-field 512.

## Update

Tool: `contacts_update`

Requires `id`. Partial update — omitted fields are left alone.

### Scalar fields (direct assignment)

Any of these may be supplied to overwrite the stored value:
`contact_type`, `name_prefix`, `first_name`, `middle_name`, `last_name`, `name_suffix`, `nickname`, the four `phonetic_*` fields, `organization`, `department`, `job_title`, `note`, `birthday`.

### Phones, emails, URLs, postal addresses (paired `_from` / `_to` ops)

Four field families are edited via paired fields:

- `phone_from` / `phone_to`
- `email_from` / `email_to`
- `url_from` / `url_to`
- `postal_from` / `postal_to`

Semantics per pair:

| `_from` | `_to` | Effect |
|---------|-------|--------|
| unset | set | **Add** `_to` as a new entry with the default label |
| set | unset | **Remove** the entry matching `_from` |
| set | set | **Replace**: match on `_from`, overwrite with `_to`, keep the original label |

Matching is **exact** against the stored value, so call `contacts_get` first to see the canonical form. `postal_from` matches against the joined-string shown in `Contact.addresses` (e.g., `"123 Main St, Austin, TX, 78701, USA"`), while `postal_to` is a structured `PostalAddressInput` (same shape as `postal_address` on create). Any `_from` or `_to` provided must be non-empty — a deliberate "clear this entry" has to be a Remove (`_from` only), not a Replace to an empty string.

Workflow:
1. `contacts_search` with the user's description to resolve the id.
2. If 0 matches: confirm spelling. If >1: disambiguate with `contacts_get` on each or ask the user.
3. `contacts_get` to read the stored form of any phone/email/URL/address you're about to replace or remove.
4. `contacts_update` with `id` + changed scalars and/or `_from` / `_to` pairs.
5. Returned JSON: `{"success": true|false}`.

Examples:

Add a second phone:
```json
{ "id": "ABCD-1234", "phone_to": "+1-555-0101" }
```

Remove an email:
```json
{ "id": "ABCD-1234", "email_from": "old@example.org" }
```

Replace a URL (label preserved):
```json
{ "id": "ABCD-1234", "url_from": "https://old.example.org", "url_to": "https://new.example.org" }
```

Replace a postal address:
```json
{
  "id": "ABCD-1234",
  "postal_from": "123 Main St, Austin, TX, 78701, USA",
  "postal_to": { "street": "456 Elm St", "city": "Austin", "state": "TX", "postal_code": "78702", "country": "USA" }
}
```

Change a scalar alongside a phone replacement:
```json
{
  "id": "ABCD-1234",
  "job_title": "Senior Mathematician",
  "phone_from": "+1-555-0100",
  "phone_to": "+1-555-0200"
}
```

## Delete

Tool: `contacts_delete`

Requires `id`. Deletion is permanent through this MCP — there is no undo tool. Before deleting:

1. Resolve id via `contacts_search`.
2. Call `contacts_get` and show the user the full record (name, organization, emails, phones).
3. Confirm with the user before calling `contacts_delete`. Do not delete on the first request for a named contact without confirmation.

Returned JSON: `{"success": true|false}`.

## Groups (read-only)

`groups_list` returns every group with member counts; `groups_members` returns contacts in a named group (case-insensitive). This MCP cannot create groups, rename them, or add/remove members — those require Contacts.app.

## Permission prompt

The first tool call after the MCP server is registered triggers macOS TCC (Transparency, Consent, and Control) to prompt the user to grant Contacts access. If they deny, subsequent calls return an internal error. Recovery path:

1. Open **System Settings → Privacy & Security → Contacts**.
2. Enable access for the process name (typically `cosmic-grackle` or the parent MCP client, depending on how the server is invoked).
3. Restart the MCP client (Claude Desktop, Claude Code, etc.) so the MCP server relaunches with the new entitlement.

## Common pitfalls

- **Treating names as stable keys.** Two contacts can share a name. Always resolve to an id before mutating, and confirm with the user when search returns more than one match.
- **Skipping `contacts_get` before a Replace or Remove.** The `_from` value must exactly match the stored form. If the user says "change Bob's 555-0100 to 555-0200", read the contact first — it may be stored as `"+1 (555) 010-0000"` and an unnormalized `_from` will silently fail to match.
- **Accidental Add when an update was intended.** Supplying only `_to` creates a new entry with the default label; it does not modify an existing one. To change an existing entry, always pair `_from` with `_to`.
- **Expecting label changes.** Add/Replace uses default labels (mobile / work / homepage / home); this MCP cannot retitle an entry's label. If the user wants a different label, they need Contacts.app.
- **Skipping the confirmation step on delete.** Deletion cannot be undone through this plugin.
- **Assuming read-after-write consistency timing.** After `contacts_create`, `contacts_search` for the same name usually returns the new contact immediately, but if it doesn't, fall back to the id returned from `contacts_create`.
