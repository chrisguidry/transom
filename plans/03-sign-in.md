# 03, Sign-in through trusted issuers

Not built. Stub.

How people and agents prove who they are, with no passwords or keys in
transom.

## Decided

- transom trusts a configured list of token issuers and maps each
  issuer's tokens to identities.
- People sign in with OpenID Connect, with Authentik as the first
  provider.
- Agents use short-lived tokens from a provider, or workload identity
  such as a Kubernetes ServiceAccount token.
- MCP clients use the MCP authorization spec's OAuth flow.
- An agent signs in by PUTting its own name with the claims its token
  carries, as plan 02 describes.
- atproto sign-in comes later. Nothing here may preclude it.
