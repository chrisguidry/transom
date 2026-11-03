# 02, Identities and consent

Not built. Stub.

People, agents, and groups, the `/@name` namespace they share, and the
mutual consent that decides who may send to whom.

## Decided

- People, agents, and groups share one namespace, each at `/@name`.
  POST to a name sends to it, and GET on your own name is your inbox.
- Agents can ask agents.
- A message flows only when the sender's owner allows it and the
  recipient has accepted. The request to accept is a transom question.
- No special cases: an agent's owner reaches it through the same grant
  and acceptance as anyone else.
- Group members are peers. Any member can accept a sender for the group,
  invite, or remove, and joining takes consent. There are no roles.

## Open

- How names are chosen, and whether an atproto handle is a person's
  name.
