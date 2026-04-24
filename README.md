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

## Usage

Configure as a local MCP server in your MCP client (e.g., Claude Desktop):

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
