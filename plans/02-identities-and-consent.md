# 02, Identities and consent

Built.

Individuals and groups, the `/@name` namespace they share, and the
mutual consent that decides who may send to whom.

## Decided

- There are two kinds of identity, individuals and groups. An
  individual is a person or an agent, and transom does not tell them
  apart.
- Individuals and groups share one namespace, each at `/@name`.
  POST to a name sends to it, and GET on your own name is your inbox.
- An individual's name defaults to a claim from its token, unless it
  chose one with a PUT, and it can change it with a PATCH to its
  `/@name`.
- Grants belong to the identity, not the name, so a rename keeps every
  consent. The old name answers `308 Permanent Redirect` to the new one
  until someone else claims it, and a new holder of the name gets none
  of the old grants.
- Agents can ask agents.
- No individual owns another. An individual is bound to its token's
  issuer and subject. A PUT to a free name from a token with no
  individual creates one under that name; a taken name the token does
  not prove gets `403 Forbidden`.
- A sender asks to reach a recipient, and a message flows only after the
  recipient accepts. A stranger's first message creates its exchange in
  the `pending` state, held. transom then sends the recipient a consent
  exchange of its own, which links to the held one, with the choices
  Allow and Decline. Allowing opens and delivers the held exchange;
  declining ends it as `declined`, which the sender learns like any other
  outcome. For a group, the first member to reply decides.
- transom has an identity of its own, `@transom`, which it registers at
  startup before it accepts any request, so no one else can claim the
  name. Every identity has accepted `@transom` from the start. It sends
  consent requests and the server's other notices.
- Every identity signs in with a token from one of the operator's
  trusted issuers, so no identity exists that those issuers do not
  vouch for.
- An individual creates a group by PUTting a free name, and becomes its
  first member.
- Group members are peers. Any member can accept a sender for the group,
  invite, or remove, and joining takes consent. There are no roles.
- `PUT /@{group}/members/@{name}` invites: `@transom` asks the invitee to
  consent, and they join when they allow it. `DELETE` on the same URL
  removes a member, or is how a member leaves.
- When the last member leaves, the group is deleted with every exchange
  it received, and its name is free again. Those exchanges' URLs answer
  `410 Gone`.
- `GET /@{name}` describes the identity, the same for anyone who asks.
  `GET /@{name}/exchanges` lists its exchanges, sent and received, and
  can be filtered, as in `?state=open`; with
  `Accept: text/event-stream` it is a live feed. Only that individual,
  or a group's members, may read it.

## Built

- `PUT`, `GET`, and `PATCH` on `/@{name}`, in `server/src/identities/`. Any
  other path at the root is not found.
- A name follows the rules for a DNS hostname: labels of lowercase
  letters, digits, and hyphens, joined by dots, up to 253 characters. An
  atproto handle is a valid name. A name claim is lowercased before it
  is checked.
- A first request whose name claim is missing, invalid, or taken gets
  `409 Conflict`, telling the caller to choose a name with a PUT.
- One Lua script looks up a name and claims it in a single step, so two
  requests racing for a free name cannot both get it.
- Responses describe an identity as JSON, `{"name": ..., "kind":
  "individual" | "group"}`, and failures as `{"error": ...}`.
- Sending to yourself needs no consent, and neither does a member sending
  to its own group: joining was its consent.
- A reply to one of `@transom`'s consent requests or invitations must name
  one of its choices, since it asks for a decision.
- `@transom` is registered by the maintenance pass that runs once a
  second, rather than once at startup, so transom starts while Redis is
  down and registers as soon as Redis is back.
- Messages from the same sender held for one recipient share a single
  consent request, and invitations to a group share a single invitation
  per invitee.
- The live feed sends a `gone` event for each exchange of a group that is
  deleted, so a client can drop it.
- A live feed subscribes to its groups' channels when it opens. Creating,
  joining, or leaving a group, or being removed from one, publishes
  `groups` on the individual's own channel, and the individual's feeds
  end when they hear it, after the `gone` events of a group deleted with
  its last member. Their clients reconnect and subscribe to the groups
  they have now. A group's feed also subscribes to its reader's channel,
  and ends when the reader leaves the group.

## Not planned

- Exchanges sent to `@transom` itself. It is an identity and could
  receive them, but nothing is planned for it.
- A limit on consent requests from a trusted identity. Every identity
  comes from an issuer the server trusts, so one that spams is a
  problem for whoever runs that issuer, and the person can decline.
