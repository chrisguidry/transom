# The wake-up gateway

Any transom server asks the gateway to wake a phone that runs the
official app, and the gateway sends that phone an FCM message with
nothing in it, so the app syncs from its own server. Plan 10 is the
design, and its Built section is the HTTP contract.

## Building and running

```sh
cargo run --package transom-gateway
```

It reads its settings from the environment:

- `TRANSOM_GATEWAY_LISTEN`, the address to listen on, by default
  `0.0.0.0:8080`.
- `TRANSOM_GATEWAY_CREDENTIALS`, the Google service account key file,
  by default `/var/run/secrets/fcm/service-account.json`. Without the
  file, the gateway answers every wake-up with `503`.
- `TRANSOM_GATEWAY_FCM_URL`, FCM's API, by default
  `https://fcm.googleapis.com`. Only tests change it.

The image builds from the root of the repository:

```sh
docker build -f gateway/Dockerfile .
```

A push to main that touches this directory publishes the image to
`ghcr.io/chrisguidry/transom-gateway` and `deploy/` to
`ghcr.io/chrisguidry/transom-gateway-deploy`, both at a tag made of the
build's UTC time and short commit (`.github/workflows/gateway.yaml`).
