# 02, Identities and consent

Partly built. Individuals, groups, names, and renames are built; consent
and group membership beyond a group's creator come with messages.

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
  recipient accepts. The request is a transom question.
- Every identity signs in with a token from one of the operator's
  trusted issuers, so no identity exists that those issuers do not
  vouch for.
- An individual creates a group by PUTting a free name, and becomes its
  first member.
- Group members are peers. Any member can accept a sender for the group,
  invite, or remove, and joining takes consent. There are no roles.

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

## Open

- Protection against a trusted identity that spams consent requests.
