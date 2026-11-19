# 21, An apt repository for the desktop app

Closed 2026-11-19. Built: all of it.

`apt upgrade` updates the desktop app with the rest of the system.

## The problem

Plan 20 ships the desktop app as a `.deb` in each GitHub release. apt
installs it, and then knows of no newer one: a person downloads the
next package by hand, or runs something of their own that follows the
releases. The app needs what the server gains in the same release, so
an app that falls behind is one that stops working.

## Decided

- The repository is at `https://transom.sh/apt`, on the GitHub Pages
  site that serves transom.sh. It needs no server of its own, and apt
  reads plain files over HTTPS.
- It delivers updates and is not an archive. It holds one package, the
  newest release's, in one suite, `stable`, with one component, `main`,
  for amd64. `apt install transom-desktop=<an older version>` does not
  work; each GitHub release keeps its own package for anyone who wants
  an older one.
- The GitHub releases are the record of what shipped, and the
  repository is built from them. A Pages deploy replaces the whole site
  and keeps nothing from the deploy before, so every deploy builds the
  repository again from the release's package, after checking the
  package's checksum and build provenance.
- The Site workflow is the only thing that deploys Pages, and it runs
  after every release as well as when the site's own files change. A
  release that changes nothing on the site still deploys it, with the
  new package.
- One suite serves every distribution. The package is built on Ubuntu
  26.04 and names the libraries it needs, so apt refuses it where they
  are older. A second build for another distribution would be a second
  suite.
- The package subscribes the machine. It installs
  `/etc/apt/sources.list.d/transom.sources` and the key that the source
  names, `/usr/share/keyrings/transom-archive-keyring.asc`, so
  installing any one release is all the setup there is. The source is a
  conffile: a person who edits or removes it keeps their change, and
  `apt purge` removes it.
- The key is served at `transom.sh/apt/transom-archive-keyring.asc`
  too, for a person who would rather add the source first and let apt
  install the package.
- The index is also published under `by-hash/`, and the `Release` file
  says so. Pages caches each file for ten minutes, so after a release a
  machine can be given the new `Release` with the old `Packages`, whose
  hash it refuses. A file fetched by its hash is the one that `Release`
  names. A machine that is given the old `Release` in that time finds
  that its index is gone and fails that one `apt update`.

## The signing key

apt trusts the repository through an OpenPGP key that signs the
`Release` file. It is separate from the build provenance attestation,
which says where a package was built: apt checks neither GitHub nor
the attestation.

- The key is Ed25519, with the fingerprint
  `0C94 8EC6 46FE 9B44 ABFB 73C7 B8DF 7C6F 796A A1AB`. Its public half
  is `desktop/apt/transom-archive-keyring.asc`.
- Its private half is the secret `TRANSOM_APT_SIGNING_KEY` in the
  GitHub environment `release`, which only `main` may deploy to, beside
  the Android app's keystore. The maintainer keeps one other copy
  offline. The Site workflow imports it for the one step that signs,
  and removes it.
- A pull request cannot read the secret. Its run makes a key of its
  own, signs with that, and checks the repository against it, which
  checks everything but the key.
- The key does not expire. An expiry protects people from a key that
  was lost and never revoked, at the cost of every machine's updates
  stopping on a date unless a release carrying the extended key reached
  it first. Replacing the key is the remedy for both, and works the
  same way with or without a date.
- To replace the key, add the next key to the keyring file, which may
  hold several, and release. Machines take the new keyring through apt,
  signed by the old key. Sign with the new key from a later release,
  and remove the old key from the keyring after that. A machine that
  skipped every release between the two has to install a package by
  hand once.

## What it costs

- Every release deploys the site, and every deploy of the site
  downloads the newest package and signs the repository again.
- A release whose Pages deploy fails has its package on GitHub and not
  in apt. Running the workflow again publishes it.
- The package adds an apt source to a machine without asking. Chrome,
  Signal, and VS Code do the same, and the source is a file a person
  can remove.

## Built

- `desktop/apt/build-repository` builds and signs the repository from
  one package, and `desktop/apt/check-repository` has apt read a
  repository with one key and download the package from it, with apt's
  state in a temporary directory.
- `desktop/apt/transom.sources` and
  `desktop/apt/transom-archive-keyring.asc`, which the package installs.
- `site.yaml` downloads and checks the release's package, builds the
  repository into the Pages artifact, and checks it with apt. After a
  deploy it checks `transom.sh/apt` the same way, with the key the
  package installs.
- `ci.yaml` runs the Site workflow after the release job, with the
  release's tag.
- `desktop.yaml` checks that the package installs the source as a
  conffile and the key the source names.
