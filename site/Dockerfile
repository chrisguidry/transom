# syntax=docker/dockerfile:1

# static-web-server is one static binary on a scratch image. The site
# needs nothing else.
FROM joseluisq/static-web-server:2.44.1@sha256:94618e3d17802265a21926217e6f9e7832b311b6fbf5933684001739fafed06a
COPY public/ /public/
USER 65532:65532
# The site serves /.well-known/assetlinks.json, which static-web-server
# refuses by default along with every other dotfile.
ENV SERVER_PORT=8080 \
    SERVER_ROOT=/public \
    SERVER_HEALTH=true \
    SERVER_IGNORE_HIDDEN_FILES=false
EXPOSE 8080
