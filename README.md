# cosmic-grackle

> [!WARNING]
> **This MCP can permanently modify or delete contacts in your local macOS address book.** The mutating tools (`contacts_create`, `contacts_update`, `contacts_delete`) write directly to `CNContactStore` — there is no undo. Before using this plugin, **back up your contacts** (Contacts.app → File → Export → Contacts Archive, or sync to iCloud) and **approve each mutating action individually** rather than running this in any auto-approve / YOLO mode. An LLM that misreads a request can clobber the wrong record just as easily as the right one.

A macOS Contacts MCP server written in Rust, plus a companion `macos-contacts` skill for Claude Code. Exposes the local Apple Contacts database (CNContactStore) over the Model Context Protocol (MCP) so Claude can read, search, and CRUD entries on the user's Mac. Applies only to the native macOS Contacts app — **not** Google Contacts, iCloud.com, Outlook, or third-party CRMs.

## What it does

This repo distributes two independent pieces: a stdio MCP server (the `cosmic-grackle` binary) and a skill (`macos-contacts`) delivered through the Claude Code plugin marketplace. The server exposes the contact tools; the skill teaches Claude when they apply, the safe workflow (search → confirm → mutate; confirm-before-delete), and how to recover from permission denials. Together they let a Claude conversation do things like "add Ada Lovelace to my contacts," "change Bob's mobile number to 555-0200," or "find everyone in my Family group."

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

## Install the MCP server

The server and the skill are distributed independently. You install the binary; you wire it into whichever MCP client you use; then (optionally) you install the skill so the model knows how to drive the tools safely.

Homebrew install is forthcoming and will be linked here once the formula is published.

Until then, download the per-arch tarball from the latest [GitHub release](https://github.com/ngerakines/cosmic-grackle/releases/latest):

```sh
# Apple Silicon
curl -fL -o cosmic-grackle.tar.gz \
  https://github.com/ngerakines/cosmic-grackle/releases/latest/download/cosmic-grackle-darwin-arm64.tar.gz
# or Intel
curl -fL -o cosmic-grackle.tar.gz \
  https://github.com/ngerakines/cosmic-grackle/releases/latest/download/cosmic-grackle-darwin-amd64.tar.gz

tar -xzf cosmic-grackle.tar.gz
xattr -d com.apple.quarantine ./cosmic-grackle 2>/dev/null || true
sudo mv ./cosmic-grackle /usr/local/bin/   # or anywhere on $PATH
```

Then register it with your MCP client. For **Claude Desktop**, add to `~/Library/Application Support/Claude/claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "contacts": {
      "command": "/usr/local/bin/cosmic-grackle"
    }
  }
}
```

For **Claude Code**, add to your project or user `.mcp.json`:

```json
{
  "mcpServers": {
    "contacts": {
      "type": "stdio",
      "command": "/usr/local/bin/cosmic-grackle"
    }
  }
}
```

Restart the client. The first contacts tool call triggers the macOS Contacts permission prompt; grant access in **System Settings → Privacy & Security → Contacts**.

## Install the skill (optional, Claude Code only)

The `macos-contacts` skill is a prompt-only guide that teaches Claude the safe workflow (search → confirm → mutate; confirm-before-delete; phone/email matching rules). It is **not** required for the tools to work, but it materially improves correctness on mutating operations. The skill is delivered through the Claude Code plugin marketplace:

```text
/plugin marketplace add ngerakines/cosmic-grackle
/plugin install cosmic-grackle@cosmic-grackle
```

This adds the skill text only — it does **not** install the MCP server. If you skipped the previous section, the skill will load but the contacts tools won't exist in your session.

The Claude Desktop **Settings → Connectors / Skills** panel cannot drive a local stdio MCP server; use Claude Code for the skill, or just rely on the README's pitfalls section if you only use Claude Desktop.

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
