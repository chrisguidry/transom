#!/usr/bin/env bash
# Runs a transom server for the desktop app to sign in to while you work
# on it: test-issuer and transom from this checkout, with a Redis that
# test-redis starts in Docker, or the one at TRANSOM_TEST_REDIS_URL.
# Everything stops, and the Redis container is removed, when this script
# exits.
#
#   ./dev-server.sh            # signs the browser in as chris
#   ./dev-server.sh sam        # signs the browser in as sam
#
# The issuer signs in whoever asks, with no password, so this is for a
# developer's own machine only. It prints how to send as someone else.
set -euo pipefail
cd "$(dirname "$0")/.."

person="${1:-chris}"
server_address=127.0.0.1:7800
issuer_address=127.0.0.1:7801
server="http://$server_address"
issuer="http://$issuer_address"
target="$PWD/target/debug"

cargo build --package transom --package test-issuer --package test-redis

# test-redis prints its Redis URL once Redis answers, and removes its
# container when its input closes, which is when this shell exits.
coproc redis_holder { "$target/test-redis"; }
read -r redis <&"${redis_holder[0]}"

pids=()
finish() {
    kill "${pids[@]}" 2>/dev/null || true
}
trap finish EXIT

"$target/test-issuer" \
    --listen "$issuer_address" \
    --audience transom-tests \
    --client-id transom \
    --client-secret transom-test-secret \
    --subject "$person" \
    --name "$person" \
    >/dev/null &
pids+=($!)

# None of this shell's own TRANSOM_ settings reach the server, so a
# developer's settings cannot change it.
(
    for name in $(compgen -e | grep '^TRANSOM_' || true); do
        unset "$name"
    done
    export \
        TRANSOM_LISTEN="$server_address" \
        TRANSOM_PUBLIC_URL="$server" \
        TRANSOM_REDIS_URL="$redis" \
        TRANSOM_REDIS_PREFIX="desktop-dev" \
        TRANSOM_ISSUERS__TEST__URL="$issuer" \
        TRANSOM_ISSUERS__TEST__AUDIENCE=transom-tests \
        TRANSOM_ISSUERS__TEST__NAME_CLAIM=preferred_username \
        TRANSOM_ISSUERS__TEST__CLIENT_ID=transom \
        TRANSOM_ISSUERS__TEST__CLIENT_SECRET=transom-test-secret \
        TRANSOM_ISSUERS__TEST__LABEL="Test issuer" \
        RUST_LOG="${RUST_LOG:-info}"
    exec "$target/transom" serve
) &
pids+=($!)

for _ in $(seq 100); do
    if curl --silent --fail --max-time 1 "$server/healthz" >/dev/null; then
        break
    fi
    sleep 0.1
done

cat <<EOF

transom is at $server, and the browser signs in as @$person.

To send to @$person as someone else, such as @alice:

    token=\$(curl -s '$issuer/access-token?subject=alice&name=alice')
    curl -H "Authorization: Bearer \$token" -X PUT $server/@alice
    curl -H "Authorization: Bearer \$token" -H 'Summary: Restart postgres?' \\
        -H 'Choices: "Restart", "Leave it"' -d 'It is using 94% of its memory.' $server/@$person

The first message from a new sender is held until @$person allows it.
Stop with Ctrl-C.
EOF
wait
