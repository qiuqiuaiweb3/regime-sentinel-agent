# MongoDB MCP Integration

This project uses the official MongoDB MCP Server as the MongoDB Track partner
MCP integration. The runtime service still talks to MongoDB through the Rust
driver on the hot path; MCP is intentionally kept out of that path and used by
MCP-capable agent clients for read-only market memory inspection.

Sources:

- MongoDB MCP Server overview: <https://www.mongodb.com/docs/mcp-server/overview/>
- MongoDB MCP Server getting started: <https://www.mongodb.com/docs/mcp-server/get-started/>
- Official GitHub repository: <https://github.com/mongodb-js/mongodb-mcp-server>

## Configuration

Template:

```text
mcp/mongodb.readonly.example.json
```

Create a local copy and replace the placeholder connection string:

```bash
cp mcp/mongodb.readonly.example.json mcp/mongodb.local.json
```

Do not commit `mcp/*.local.json`. It is ignored because it may contain the Atlas
connection string.

The template pins the MCP package to `mongodb-mcp-server@1.11.0` and includes
`--readOnly`. Read-only mode is the project default because the agent should not
mutate market data collections through MCP. Writes remain owned by the Rust
service and explicit CLIs.

## Direct Run

For a local MCP-capable client that inherits environment variables:

```bash
set -a
source .env
set +a
MDB_MCP_CONNECTION_STRING="${MONGODB_URI}" \
  npx -y mongodb-mcp-server@1.11.0 --readOnly
```

For a client that reads JSON config, point it at `mcp/mongodb.local.json`.

## Boundaries

- Hot path: Rust service, MongoDB Rust driver, no MCP.
- Agent Builder: hosted OpenAPI tool for Cloud Run endpoints.
- Partner MCP: official MongoDB MCP Server, read-only, for MongoDB memory
  inspection in MCP-capable agent clients.
- Secrets: `.env`, Secret Manager, or local ignored MCP config only.

## Local Verification

The local package resolution was verified with:

```bash
npx -y mongodb-mcp-server@1.11.0 --version
```

Observed version:

```text
1.11.0
```

The checked-in JSON template is valid:

```bash
node -e "JSON.parse(require('fs').readFileSync('mcp/mongodb.readonly.example.json','utf8'))"
```
