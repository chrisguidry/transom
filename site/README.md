# transom.sh

The project's website, and the DNS zone it answers from.

- `public/` is the site: static files, served by GitHub Pages at
  transom.sh.
- `dns.tf` is the transom.sh zone on Linode, run from this directory.

A push to main that touches `public/` deploys it to Pages through
`.github/workflows/site.yaml`, which uploads the directory as it is,
with no Jekyll build. The upload keeps `.well-known/assetlinks.json`,
which Android fetches to verify the app's sign-in callback,
`https://transom.sh/app/callback`, as an App Link; the workflow fails
when the file is missing from the upload. A pull request runs the same
build and check without deploying.

The repository's Pages settings name transom.sh as the custom domain
and enforce HTTPS, so GitHub issues the site's certificate. The apex
records in `dns.tf` are GitHub Pages' published addresses.
