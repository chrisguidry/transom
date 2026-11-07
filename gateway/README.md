# The wake-up gateway

Any transom server asks the gateway to wake a phone that runs the
official app, and the gateway sends that phone an FCM message with
nothing in it, so the app syncs from its own server. Plan 10 is the
design, and its Built section is the HTTP contract, except that the
gateway now answers wake-ups at `POST /` on its own host,
`https://wake.transom.sh/`.

## Building and running

```sh
cargo run --package transom-gateway
```

It reads its settings from the environment:

- `TRANSOM_GATEWAY_LISTEN`, the address to listen on, by default
  `0.0.0.0:8080`.
- `TRANSOM_GATEWAY_METADATA_URL`, the metadata server of the Cloud Run
  service the gateway runs on, by default
  `http://metadata.google.internal`. At startup the gateway reads its
  project there, and it asks there for its service account's access
  tokens to FCM. Without a metadata server, as outside Google Cloud, the
  gateway still runs and answers every wake-up with `503`. Only tests
  change it.
- `TRANSOM_GATEWAY_FCM_URL`, FCM's API, by default
  `https://fcm.googleapis.com`. Only tests change it.

The image builds from the root of the repository:

```sh
docker build -f gateway/Dockerfile .
```

## Deploying

A push to main that touches this directory publishes the image to
`ghcr.io/chrisguidry/transom-gateway`, at a tag made of the build's UTC
time and short commit (`.github/workflows/gateway.yaml`). Terraform owns
which image the Cloud Run service `gateway` runs: `image.auto.tfvars`
names its tag and digest, and an apply deploys it. Cloud Run pulls the
image by digest through the Artifact Registry repository `ghcr`, which
proxies ghcr.io.

Cloud Run's front end answers some paths that end in `z` itself, so
`/healthz` on the service's `run.app` URL is Google's 404, not the
gateway's.

## The Google Cloud project

The Terraform in this directory makes and runs the project
`transom-wake`:

- `firebase.tf` makes the project, links it to the billing account,
  turns on the APIs, Firebase, and FCM, registers the Android app
  `sh.transom`, and makes the gateway's service account, which may send
  FCM messages. Its outputs, in `outputs.tf`, are the values the app is
  built with.
- `billing.tf` sets a budget of one dollar a month on the project, which
  emails the billing account's administrators at half and all of it.
  The gateway's traffic fits in Cloud Run's free tier.
- `cloud_run.tf` makes the Artifact Registry repository and the Cloud
  Run service in `us-central1`. The service runs as the gateway's
  service account, scales from zero to at most one instance, because the
  per-phone rate limit lives in memory, and lets anyone invoke it.
  It runs the image `image.auto.tfvars` names.
- `domain.tf` verifies transom.sh in Site Verification with a TXT
  record, maps `wake.transom.sh` to the service, and points it at Google
  with a CNAME. site/ owns the transom.sh zone; this directory writes
  only these records into it. Google issues the mapping's certificate,
  which site/'s CAA record allows.
- `github_actions.tf` makes the Workload Identity Federation pool for
  GitHub Actions and the service accounts its workflows act as.

## Applying it

Applying needs a person, run from this directory:

1. Sign in to Google for Terraform, with the scope Site Verification
   needs beside cloud-platform:

   ```sh
   gcloud auth application-default login \
     --scopes=https://www.googleapis.com/auth/cloud-platform,https://www.googleapis.com/auth/siteverification
   ```

   A service account can create a project only inside an organization,
   so the first apply is a person's. The person needs to administer the
   billing account, and becomes an owner of transom.sh in Site
   Verification.
2. If the account has never used Firebase, open
   https://console.firebase.google.com, create a project there to accept
   Firebase's terms, and delete it. Without that, the apply fails with
   `Firebase Tos Not Accepted`.
3. Export the state bucket's credentials, `AWS_ACCESS_KEY_ID` and
   `AWS_SECRET_ACCESS_KEY`, from the 1Password item "transom Terraform
   state (Linode Object Storage)", and `LINODE_TOKEN` from "transom
   Linode token", as `site/` does through a gitignored `.envrc.private`.
4. `terraform init`, then `terraform apply`. Project ids are unique
   across Google Cloud; if `transom-wake` is taken, choose another with
   `-var project_id=...` and keep it in a `terraform.tfvars`.
5. Give the app build the outputs as Gradle properties.
   `terraform output -raw gradle_properties` prints the four lines of
   `transom.firebase.*`: append them to `~/.gradle/gradle.properties`
   for local builds, and set them as CI variables for CI's builds.
   None of them is secret.

Google issues the domain mapping's certificate after the CNAME
resolves, which can take up to half an hour. Until then,
`https://wake.transom.sh/` refuses TLS connections.

A new signing key for the app means adding its SHA-256 fingerprint to
`android_signing_sha256` and to `site/public/.well-known/assetlinks.json`,
then applying again.
