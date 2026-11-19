# transom.sh

The project's website, and the DNS zone it answers from.

- `public/` is the site: static files, served by GitHub Pages at
  transom.sh.
- transom.sh/apt is the apt repository that delivers the desktop app's
  updates. It is in no directory here: each deploy builds it from the
  newest GitHub release with `../desktop/apt/build-repository`.
  [Plan 21](../plans/completed/21-apt-repository.md) is its design.
- `dns.tf` is the transom.sh zone on Linode, and `repository.tf` is
  the chrisguidry/transom repository's settings and its Pages site.
  Terraform runs both from this directory.

A push to main that touches `public/`, and every release, deploys the
site to Pages through `.github/workflows/site.yaml`, which uploads the
directory as it is, with no Jekyll build, and the apt repository beside
it. The upload keeps `.well-known/assetlinks.json`, which Android
fetches to verify the app's sign-in callback,
`https://transom.sh/app/callback`, as an App Link; the workflow fails
when the file is missing from the upload, and when apt cannot take the
desktop app's package from the repository. A pull request runs the
same build and checks without deploying.

`repository.tf` names transom.sh as the Pages site's custom domain and
enforces HTTPS, and GitHub issues and renews the site's certificate.
The apex records in `dns.tf` are GitHub Pages' published addresses.
