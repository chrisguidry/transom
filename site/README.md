# transom.sh

The project's website, and the DNS zone it answers from.

- `public/` is the site: static files, served by
  [static-web-server](https://static-web-server.net) from the image that
  `Dockerfile` builds.
- `deploy/` is the Kubernetes deployment: the Deployment, its Service,
  and an Ingress for transom.sh whose certificate comes from Let's
  Encrypt through cert-manager.
- `dns.tf` is the transom.sh zone on Linode, run from this directory.

A push to main that touches this directory publishes the image to
`ghcr.io/chrisguidry/transom-site` and this directory's `deploy/` to
`ghcr.io/chrisguidry/transom-site-deploy`, both at a tag made of the
build's UTC time and short commit (`.github/workflows/site.yaml`). The
cluster that serves the site follows the newest tag of the artifact
with Flux, and the A record in `dns.tf` names that cluster's address.
