# 07, The MCP server

Not built. Stub.

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
