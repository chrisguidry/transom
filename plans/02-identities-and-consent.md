# 02, Identities and consent

Not built. Stub.

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

## Open

- Protection against a trusted identity that spams consent requests.
