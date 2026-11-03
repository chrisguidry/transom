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
- A token's issuer and subject map to exactly one individual. A tool
  someone signs in to with their own account acts as them.
- The first request from a token with no individual creates one: under
  the name it PUTs, or else under the issuer's name claim.
- Issuers are configured with config-rs, from a YAML file, environment
  variables such as `TRANSOM_ISSUERS__AUTHENTIK__URL`, or both.
- Tokens are verified with the `jsonwebtoken` crate, and issuers'
  keys are discovered with the `openidconnect` crate.
- atproto sign-in comes later. Nothing here may preclude it.
