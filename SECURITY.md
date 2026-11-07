# Security policy

## How to report a vulnerability

Report a vulnerability privately, through GitHub's advisory form:
[Report a vulnerability](https://github.com/chrisguidry/transom/security/advisories/new).
The report opens a draft advisory that only you and the maintainer can
see. Do not open a public issue for a vulnerability.

Expect an acknowledgment within a week. transom has one maintainer, so
there is no on-call rotation behind that number.

## What is in scope

transom's own code is in scope: the server, the wake-up gateway, the
Android app, and the release workflow. The wake-up gateway at
wake.transom.sh is also in scope, because the official app trusts the
wake-ups it sends.

## How fixes ship

A fix ships in a new release. The newest release is the supported one.
