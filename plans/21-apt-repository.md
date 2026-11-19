# 21, An apt repository for the desktop app

Not built. Stub.

Plan 20 ships the desktop app as a `.deb` on GitHub releases, which a
person downloads again for each update. An apt repository lets
`apt upgrade` update it with the rest of the system.

## Open

- Where the repository is served from: transom.sh, which is GitHub
  Pages, or a release's assets.
- The key that signs the repository's `Release` file, where it is kept,
  and how a person adds it, as a `signed-by` keyring beside a
  `.sources` file.
- Whether the `.deb` installs the `.sources` file itself, so that
  installing one release subscribes the machine to the rest.
- Which Ubuntu and Debian releases the package is built for, since it
  links the system's GTK 4 and libadwaita.
- How many releases the repository keeps.
