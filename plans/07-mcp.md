# 07, The MCP server

Built.

The MCP tools agents use to send, ask, and answer.

## Decided

- The server serves MCP itself.
- An agent can wait for an answer, or get an id at once and have the
  answer delivered to its webhook.
- Agents read and answer their own inbox through MCP.
- `/mcp` is a route on the same server. Each tool calls the same internal
  functions as the HTTP handlers, and an MCP session signs in with the
  same bearer token, so an agent's MCP calls and its HTTP calls are one
  identity.
- The tools are cut by task, so their names and descriptions tell a model
  whether it is interrupting someone or informing them:
  - `ask` sends a question, with choices, a prompt, or both, and can wait
    for the reply.
  - `notify` sends a one-way notification.
  - `check` gives an exchange's state and reply, and can wait.
  - `reply` replies to an exchange in the agent's own inbox.
  - `withdraw` takes back a question.
  - `inbox` lists the agent's own exchanges, open ones first.

## Built

- `server/src/mcp/` serves the tools with rmcp's Streamable HTTP transport.
  A layer in front of it signs each request in the same way every HTTP
  route does and hands the caller to the tools.
- `ask` and `check` take a `wait` in seconds. While they wait, they send a
  progress notification every ten seconds to a client that asked for
  them, so the client's own timeout does not end the call.
- A tool reports what went wrong as an error result, which the model
  reads, rather than as a protocol error.
- The tool parameters' descriptions come from the doc comments on the
  message's fields, which tell the model what each field is for.
- rmcp's check of the `Host` header against localhost is off: it guards a
  local server against DNS rebinding, and every request to `/mcp` carries
  a bearer token, which a rebinding page cannot attach.
- Sessions are kept in memory, so the server runs one replica.
