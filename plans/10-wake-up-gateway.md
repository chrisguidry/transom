# 10, The wake-up gateway

Not built. Stub.

How a self-hosted server wakes the official app through Firebase Cloud
Messaging.

## Decided

- The project runs the gateway for the official app.
- A wake-up carries no title, body, or sender. The app wakes and syncs
  from its own server.
- The gateway holds no state.
- Phones without Google services can hold a connection open to the
  server instead.
