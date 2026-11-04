# 08, The web UI

Not built. Stub.

The inbox in a browser, and where people manage devices, agents,
groups, and consent.

## Decided

- The server serves it.
- Each message has its own page, so other channels can link to it.
- The web UI is the API's own URLs, served as HTML to any client that
  prefers `text/html` in its `Accept` header: the inbox at
  `/@{name}/exchanges`, each exchange at `/exchanges/{id}` with its choices
  and text box as a form, and a group's members at `/@{group}/members`.
- The pages are rendered on the server, with htmx. htmx sends the real
  methods, `PUT` for a reply and `DELETE` for a withdrawal, which a plain
  HTML form cannot, and its SSE extension takes live updates from the
  same event streams the API serves.
- Signing in follows RFC 10017 (BCP 212), OAuth 2.0 for Browser-Based
  Applications, in its most secure pattern, the Backend for Frontend.
  transom is a confidential client: it runs the authorization code flow
  with PKCE against the issuer, keeps the tokens on the server in a
  session in Redis, and gives the browser only a session cookie, named
  with the `__Host-Http-` prefix and set `Secure`, `HttpOnly`, and
  `SameSite=Strict`, with path `/` and no `Domain`.
- Cross-site requests are refused by requiring a custom request header,
  which htmx sends on every request as `HX-Request`.
- Each issuer people sign in with in a browser has a client credential:
  `private_key_jwt` (RFC 7523) where the issuer supports it, and a client
  secret otherwise. openidconnect runs the flow.
