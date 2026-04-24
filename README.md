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

## Install into Claude Desktop

Each tagged release publishes `cosmic-grackle.mcpb` — an [MCP Bundle](https://github.com/anthropics/mcpb) containing a universal (x86_64 + arm64) macOS binary and a manifest.

1. Download `cosmic-grackle.mcpb` from the latest release.
2. Remove Gatekeeper quarantine so Claude Desktop can launch the unsigned binary after install:
   ```sh
   xattr -d com.apple.quarantine cosmic-grackle.mcpb
   ```
3. Double-click the `.mcpb` file (or drag it into Claude Desktop's Extensions panel) to install.

Claude Desktop extracts the bundle and registers the MCP server. The first tool call triggers the macOS Contacts permission prompt; grant access in **System Settings → Privacy & Security → Contacts**.

## Usage (manual config)

As an alternative to the MCPB bundle, configure the binary directly in your MCP client:

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
