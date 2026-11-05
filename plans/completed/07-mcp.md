# 07, The MCP server

Closed 2026-11-05. Built.

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
- The service keeps no sessions: each request stands alone, so any
  replica can serve it (plan 17).
- An MCP client finds how to sign in from OAuth 2.0 Protected Resource
  Metadata (RFC 9728), as the MCP authorization spec requires. A `401`
  from `/mcp` carries `resource_metadata` in its challenge, pointing to
  `/.well-known/oauth-protected-resource/mcp`, whose `resource` is the
  public origin followed by `/mcp`. A `401` from the HTTP API points to
  `/.well-known/oauth-protected-resource`, whose `resource` is the
  origin alone. RFC 9728 requires a client to refuse a document whose
  `resource` differs from the URL it called, so each document names its
  own resource.
- The document's `authorization_servers` are the issuers with a
  `client_id`: those transom signs people in with. An issuer that only
  verifies tokens, like a Kubernetes cluster, has no sign-in for a client
  to start. Its `scopes_supported` are the scopes browser sign-in asks
  for, `openid profile email offline_access`: `profile` and `email`
  carry the claims a `name_claim` usually names, and `offline_access`
  gets the client a refresh token. The document may be cached for an
  hour.
- A claude.ai connector signs in as transom's own client, with the
  client ID and secret entered in the connector's advanced settings,
  because Authentik does not offer dynamic client registration. The
  issuer must allow the redirects `https://claude.ai/api/mcp/auth_callback`
  and `https://claude.com/api/mcp/auth_callback`. The README's steps
  give the rest.
- Authentik issues the connector's access token to the client, so its
  `aud` is the client ID, and the issuer's `audience` must equal the
  `client_id`. MCP clients also send the `resource` parameter of RFC
  8707. An issuer that honors it would put the resource's URL in `aud`
  instead, and those tokens would be refused until the `audience` is set
  to that URL. transom accepts one audience per issuer, which keeps a
  token issued for another service from being accepted.
