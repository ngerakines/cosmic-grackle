# cosmic-grackle

> [!WARNING]
> **This MCP can permanently modify or delete contacts in your local macOS address book.** The mutating tools (`contacts_create`, `contacts_update`, `contacts_delete`) write directly to `CNContactStore` — there is no undo. Before using this plugin, **back up your contacts** (Contacts.app → File → Export → Contacts Archive, or sync to iCloud) and **approve each mutating action individually** rather than running this in any auto-approve / YOLO mode. An LLM that misreads a request can clobber the wrong record just as easily as the right one.

A macOS Contacts MCP server and Claude Code plugin written in Rust. Exposes the local Apple Contacts database (CNContactStore) over the Model Context Protocol (MCP) so Claude can read, search, and CRUD entries on the user's Mac. Applies only to the native macOS Contacts app — **not** Google Contacts, iCloud.com, Outlook, or third-party CRMs.

## What it does

The plugin ships two pieces: a stdio MCP server (`contacts`) and a skill (`macos-contacts`). The skill teaches Claude when the tools apply, the safe workflow (search → confirm → mutate; confirm-before-delete), and how to recover from permission denials. Together they let a Claude conversation do things like "add Ada Lovelace to my contacts," "change Bob's mobile number to 555-0200," or "find everyone in my Family group."

Every contact carries an opaque `id`. **Updates and deletes work by id, never by name** — two people can share a name, so the workflow always resolves an id with `contacts_search` (or `contacts_list`) and disambiguates with the user before mutating. `contacts_get` returns the canonical stored form of a record, which matters because the `_from`/`_to` update ops match `_from` exactly against the stored value.

What it can't do: rename entry labels (e.g., switch a phone from "mobile" to "work"), add or remove members from a group, or operate on contact stores other than the local Mac. Those require Contacts.app or a different integration.

## Requirements

- macOS (uses the native Contacts framework)
- Rust 1.95+ (only when building from source)
- Contacts access permission (prompted on first tool call; granted to bundle ID `me.ngerakines.cosmic-grackle`)

## Building

```sh
cargo build --release
```

## Install as a Claude plugin

Install through the Claude plugin marketplace from inside Claude Code (works for both Claude Desktop and the standalone Claude Code CLI — they share `~/.claude/plugins/`):

```text
/plugin marketplace add ngerakines/cosmic-grackle
/plugin install cosmic-grackle@cosmic-grackle
```

On the next session start, a `SessionStart` hook downloads the universal (x86_64 + arm64) binary from the matching GitHub release into the plugin's `bin/` directory and clears the macOS Gatekeeper quarantine. The first contacts tool call then triggers the macOS Contacts permission prompt; grant access in **System Settings → Privacy & Security → Contacts**.

To upgrade, run `/plugin update cosmic-grackle@cosmic-grackle` and restart the session — the hook detects the version change and re-fetches the binary.

## Usage (manual config)

As an alternative to the plugin bundle, configure the binary directly in your MCP client (e.g., Claude Desktop):

```json
{
  "mcpServers": {
    "contacts": {
      "command": "/path/to/cosmic-grackle"
    }
  }
}
```

## Tools

### Read-only

| Tool | Purpose |
|------|---------|
| `contacts_list` | List contacts, optionally capped by `limit`. Returns full records including the opaque `id` required by update / delete. |
| `contacts_search` | Search by name substring. Primary way to resolve an `id`. If multiple matches, the workflow asks the user which one. |
| `contacts_get` | Read a single contact by `id` in canonical stored form. Call this before any `_from`/`_to` update so the match value is exact, and before any delete to show the user what's about to be removed. |
| `groups_list` | List every group with member counts. |
| `groups_members` | Return contacts in a named group (case-insensitive). |

### Mutating

| Tool | Purpose |
|------|---------|
| `contacts_create` | Create a contact. At least one of `first_name`, `last_name`, or `organization` is required. Accepts a full set of name / phonetic / organization fields plus one each of `phone`, `email`, `url`, and `postal_address` (each gets the default label — mobile / work / homepage / home). Use `contacts_update` afterward to add more entries. |
| `contacts_update` | Partial update by `id`. Scalar fields (names, organization, job_title, note, birthday, etc.) overwrite directly. Phones, emails, URLs, and postal addresses use paired `phone_from`/`phone_to`, `email_from`/`email_to`, `url_from`/`url_to`, `postal_from`/`postal_to` ops: `_to` only adds, `_from` only removes, both replaces in place. Matching is exact against the stored form. |
| `contacts_delete` | Delete a contact by `id`. Permanent — the workflow shows the full record and gets explicit user confirmation first. |

See [plugin/skills/macos-contacts/SKILL.md](plugin/skills/macos-contacts/SKILL.md) for the full workflow, length caps, and worked examples.

## Permissions

The first tool call after install triggers the macOS Contacts permission prompt. If access is denied, subsequent calls return an internal error. To recover: open **System Settings → Privacy & Security → Contacts**, enable access for the invoking process (typically `cosmic-grackle` or the parent Claude app), then restart Claude so the MCP server relaunches with the new entitlement.

## License

Licensed under the [MIT License](LICENSE).
