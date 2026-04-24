---
name: macos-contacts
description: Use when the user wants to create, update, or delete a contact in the macOS Contacts app (Apple Contacts) on their local Mac. Triggers on phrases like "add a contact on my Mac", "create a new contact for X", "update Jane's job title", "change Bob's organization", "delete the contact for Alice", "remove a duplicate contact from Contacts.app". Applies only to the native macOS Contacts app backed by the CNContactStore framework — NOT Google Contacts, iCloud.com, CRMs, or address books in third-party apps. Uses the stdio MCP server registered as `contacts` in this plugin, which exposes `contacts_search`, `contacts_get`, `contacts_create`, `contacts_update`, `contacts_delete`, `contacts_list`, `groups_list`, and `groups_members`. The first tool invocation after install prompts the user to grant Contacts access in System Settings.
---

# macOS Contacts CRUD

This skill covers create / update / delete workflows against the macOS Contacts app via the `contacts` MCP server. Read-only tools (`contacts_list`, `contacts_search`, `contacts_get`, `groups_list`, `groups_members`) are documented here only insofar as they are prerequisites for mutating operations.

## When to use

- The user is on macOS and wants to modify the local Contacts database.
- The request names a specific contact by name or describes one to be created.

## When NOT to use

- The user is managing contacts in Google, iCloud web, Outlook, HubSpot, Salesforce, or any non-macOS store.
- The user wants to edit fields this MCP does not expose: phone numbers on an existing contact, email addresses on an existing contact, birthdays, addresses, or group membership.

## Identifier model

Every contact has an opaque identifier returned in the `id` field. `contacts_update` and `contacts_delete` require this id — they do NOT accept a name.

Before any update or delete, resolve the id with `contacts_search` (by name substring) or `contacts_list`. If the search returns multiple matches, ask the user which one to act on; do not guess.

`contacts_get` returns the full contact record for a known id and is useful for confirming "am I about to edit the right person?" before a mutation.

## Create

Tool: `contacts_create`

At least one of `first_name`, `last_name`, or `organization` is required. All other fields are optional. `email` and `phone` are the only array-backed fields exposed on create — if multiple phones or emails are needed, you can only populate one via this tool; the user must add the rest in Contacts.app manually.

Returned JSON: `{"id": "<opaque-id>", "success": true}`. Store the id if subsequent operations are planned.

Example call:

```json
{
  "first_name": "Ada",
  "last_name": "Lovelace",
  "email": "ada@example.org",
  "phone": "+1-555-0100",
  "organization": "Analytical Engine Co.",
  "job_title": "Mathematician",
  "note": "First programmer"
}
```

Length caps (MCP rejects over-limit input): names 256 bytes, email 320, phone 64, organization 256, job title 256, note 4096.

## Update

Tool: `contacts_update`

Requires `id`. Supply only the fields you want to change — omitted fields are left alone. This is a partial update, not a replace.

**Updatable fields**: `first_name`, `last_name`, `organization`, `job_title`, `note`.

**Not updatable via this MCP**: `email`, `phone`, birthday, addresses, group membership. If the user asks to change any of these, tell them the MCP cannot do it and they need to edit in Contacts.app directly.

Workflow:
1. `contacts_search` with the user's description (e.g., name) to resolve the id.
2. If 0 matches: confirm spelling with the user. If >1: disambiguate using `contacts_get` on each or ask the user.
3. `contacts_update` with `id` + the fields to change.
4. Returned JSON: `{"success": true|false}`.

Example call:

```json
{
  "id": "ABCD-1234-...",
  "job_title": "Senior Mathematician",
  "organization": "Babbage & Co."
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

The first tool call after installing this plugin triggers macOS TCC (Transparency, Consent, and Control) to prompt the user to grant Contacts access. If they deny, subsequent calls return an internal error. Recovery path:

1. Open **System Settings → Privacy & Security → Contacts**.
2. Enable access for the process name (typically `cosmic-grackle` or the parent Claude app, depending on how the MCP is invoked).
3. Restart Claude so the MCP server relaunches with the new entitlement.

## Common pitfalls

- **Treating names as stable keys.** Two contacts can share a name. Always resolve to an id before mutating, and confirm with the user when search returns more than one match.
- **Trying to update `email` or `phone`.** The MCP does not expose these on update. Surface that limitation to the user instead of silently doing nothing.
- **Skipping the confirmation step on delete.** Deletion cannot be undone through this plugin.
- **Assuming read-after-write consistency timing.** After `contacts_create`, `contacts_search` for the same name usually returns the new contact immediately, but if it doesn't, fall back to the id returned from `contacts_create`.
