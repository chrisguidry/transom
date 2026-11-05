# 08, The web UI

Built.

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
  secret otherwise. transom runs the flow itself, as plan 03 explains.

## Built

- `server/src/web/` renders the pages with askama templates in
  `server/templates/`. askama's optional configuration file is off, so the
  templates sit in its default directory: reading that file needs
  `basic-toml`, which is unmaintained.
- A request prefers HTML when its `Accept` lists `text/html`, or when
  htmx sends it, since htmx marks its requests with `HX-Request` and does
  not ask for HTML by `Accept`. A browser that is not signed in is sent
  to `/login`.
- Every page carries a Content Security Policy that allows only transom's
  own scripts and styles. htmx's `eval`, script tags, and inline styles
  are off, and the only script of transom's own is `transom.js`.
- htmx 2.0.11 and its SSE extension 2.2.4 are vendored in
  `server/src/web/static/`, taken from the npm registry and checked against
  its published SHA-512. Their names carry their versions, so browsers
  keep them, and Dependabot does not see them: updating one is by hand.
- Markdown is rendered with raw HTML shown as text, links kept only to
  `http`, `https`, `mailto`, and transom's own pages, and images shown as
  links rather than loaded, so a message cannot learn when it is read.
- The inbox lists what waits on the viewer first. An exchange's page
  follows its own stream with `?changes`, which leaves out the opening
  snapshot, and only while the exchange can still change; otherwise a
  page that re-renders on each event would reconnect, and re-render on
  the snapshot, without end.
- An exchange's form always has Reply under the text box, which sends
  the text with no choice, since any message takes free text. Beside
  choices it is quiet, and `transom.js` enables it only while the box
  has text; each choice still sends its choice with the text.
- The look is plain: system fonts, one muted accent, and light and dark
  schemes, with nothing that moves unless the person asked it to.
- Push notifications use Web Push: RFC 8030 to the browser's push
  service, encrypted per RFC 8291 so the push service sees only
  ciphertext, and signed with a VAPID key per RFC 8292, which transom
  keeps in Redis. A browser is a device of plan 09's kind, waking by
  `web_push`, and the inbox offers a button that subscribes or
  unsubscribes it. Each exchange that arrives for the identity, or for a
  group it is in, is pushed with its summary, its sender and the start of
  its body, and its first two choices as buttons that reply from the
  notification. A second push closes the notification when the exchange
  ends, and its `Topic` replaces the first at a push service still
  holding it. Pushes queue in Redis like webhooks, and are retried for an
  hour while the push service does not take them; a push service that
  answers `404` or `410` has forgotten the subscription, and transom
  forgets the device.
- Marking an exchange read pushes `{"event": "read"}` with the same
  `Topic` to the reader's own devices only, and the service worker
  closes the notification as it does on `ended`. Marking it unread
  pushes nothing.
