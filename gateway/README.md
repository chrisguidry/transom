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

## The Firebase project

`firebase.tf` makes the Google Cloud project `transom-wake`, turns on
Firebase and FCM, registers the Android app `sh.transom`, and makes the
gateway's service account. It writes that account's key to the Secret
`gateway-fcm` in the `transom` namespace of the public-sites cluster,
under `service-account.json`. Its outputs are the values the app is
built with.

## Applying it

Applying needs a person, run from this directory:

1. Sign in to Google for Terraform with
   `gcloud auth application-default login`. To use a service account's
   key instead, set `GOOGLE_APPLICATION_CREDENTIALS` to its path. A
   service account can create a project only inside an organization, so
   the first apply is a person's.
2. If the account has never used Firebase, open
   https://console.firebase.google.com, create a project there to accept
   Firebase's terms, and delete it. Without that, the apply fails with
   `Firebase Tos Not Accepted`.
3. Export the state bucket's credentials, `AWS_ACCESS_KEY_ID` and
   `AWS_SECRET_ACCESS_KEY`, from the 1Password item "transom Terraform
   state (Linode Object Storage)", as `site/` does through a gitignored
   `.envrc.private`.
4. Make sure `kubectl --context public-sites get namespace transom`
   answers. Terraform writes the Secret through that context of
   `~/.kube/config` (the variables `kubeconfig_path` and
   `kube_context`).
5. `terraform init`, then `terraform apply`. Project ids are unique
   across Google Cloud; if `transom-wake` is taken, choose another with
   `-var project_id=...` and keep it in a `terraform.tfvars`.
6. Give the app build the outputs as Gradle properties.
   `terraform output -raw gradle_properties` prints the four lines of
   `transom.firebase.*`: append them to `~/.gradle/gradle.properties`
   for local builds, and set them as CI variables for CI's builds.
   None of them is secret.
7. Restart the gateway, which reads its key only at startup:
   `kubectl --context public-sites -n transom rollout restart
   deployment/gateway`.

The gateway's service account key is in the Terraform state, and the
state is in the bucket. To replace the key, run `terraform apply
-replace=google_service_account_key.gateway`, then
`kubectl --context public-sites -n transom rollout restart
deployment/gateway`, since the gateway reads the key only at startup.

A new signing key for the app means adding its SHA-256 fingerprint to
`android_signing_sha256` and to `site/public/.well-known/assetlinks.json`,
then applying again.
