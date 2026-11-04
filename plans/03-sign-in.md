# 03, Sign-in through trusted issuers

Built.

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
- Tokens are verified with the `jsonwebtoken` crate.
- atproto sign-in comes later. Nothing here may preclude it.

## Built

- Each issuer has two more optional settings, for a Kubernetes cluster
  whose discovery endpoints only ServiceAccounts may read and whose keys
  are served under the cluster's own CA: `ca_file`, a CA certificate to
  trust when fetching the issuer's keys, and `token_file`, a token to send
  when fetching them, read on every fetch because projected tokens
  rotate.
- transom finds an issuer's keys itself, from `/.well-known/openid-configuration`
  and its `jwks_uri`. openidconnect's discovery requires an
  `authorization_endpoint`, which a Kubernetes cluster does not publish, so
  openidconnect waits for the web UI's browser sign-in.
- A token naming a key transom has not seen makes it fetch the keys
  again, at most once every 30 seconds.
- Tokens signed with a shared secret (HS256, HS384, HS512) are refused,
  because an issuer's keys are public.
- A request without a valid token gets `401` with the
  `WWW-Authenticate` challenge of RFC 6750. A request whose issuer
  cannot be reached gets `503`.
- jsonwebtoken and rustls both use aws-lc-rs for cryptography. The pure
  Rust backend carries two advisories with no fixed release, in `rsa`
  and in `crypto-bigint`.
- The image carries the public CA certificates, for issuers with public
  TLS certificates.
- The tests run a real issuer in-process, which serves a discovery
  document and Ed25519 keys and signs tokens with them.
- Browser sign-in does not use the `openidconnect` crate either. It
  depends on reqwest 0.12, while transom is on 0.13, and brings a second
  copy of most of the RustCrypto stack, which cargo-deny refuses. transom
  runs the authorization code flow itself, in `signin/browser.rs`, with
  the discovery, the HTTP client, and the ID token verification it
  already has.
