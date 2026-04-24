# cosmic-grackle

A macOS Contacts MCP server written in Rust. Exposes Apple Contacts via the Model Context Protocol (MCP) over stdio transport.

## Requirements

- macOS (uses the native Contacts framework)
- Rust 1.85+
- Contacts access permission (prompted on first run)

## Building

```sh
cargo build --release
```

## Install as a Claude plugin

Each tagged release publishes `cosmic-grackle-plugin.tar.gz` — a bundle containing a universal (x86_64 + arm64) binary, the `macos-contacts` skill, and a plugin manifest. Install it into Claude Desktop / Claude Code:

```sh
mkdir -p ~/.claude/plugins
tar -xzf cosmic-grackle-plugin.tar.gz -C ~/.claude/plugins/
xattr -dr com.apple.quarantine ~/.claude/plugins/cosmic-grackle
```

The `xattr` step removes Gatekeeper quarantine from the unsigned binary so Claude can launch it. Restart Claude Desktop (or run `/plugin` in Claude Code) to pick up the plugin. The first tool call triggers the macOS Contacts permission prompt; grant access in **System Settings → Privacy & Security → Contacts**.

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

| Tool | Description |
|------|-------------|
| `contacts_list` | List all contacts (optional `limit` parameter) |
| `contacts_search` | Search contacts by name |
| `contacts_get` | Get a contact by identifier |
| `contacts_create` | Create a new contact |
| `contacts_update` | Update an existing contact |
| `contacts_delete` | Delete a contact |
| `groups_list` | List all contact groups with member counts |
| `groups_members` | Get contacts in a group by name |

## License

Licensed under the [MIT License](LICENSE).
