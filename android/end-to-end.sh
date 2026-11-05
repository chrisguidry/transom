#!/usr/bin/env bash
# The end-to-end test: a person signs in to the app on an emulator through
# the Custom Tab, and answers a question from its notification.
#
# This script runs the servers on this machine: test-issuer and transom,
# with its keys under a prefix of this run's own in a Redis that test-redis
# starts in Docker, or in the Redis at TRANSOM_TEST_REDIS_URL. adb reverse
# brings each one to the same port on the emulator's 127.0.0.1, which is
# the only address a debug build may speak plain HTTP to. The ports match on both sides because the issuer's
# URL is in transom's settings and in the pages the Custom Tab opens, so
# transom and the emulator must reach the issuer at the same URL. Gradle
# then installs the app and runs the test in src/androidTest, which plays
# both the person, through UI Automator, and the asker, through HTTP.
#
#   ANDROID_SERIAL=emulator-5554 ./end-to-end.sh
#
# ANDROID_SERIAL is required, so the test never runs on a phone that
# happens to be connected too.

set -euo pipefail

: "${ANDROID_SERIAL:?name the emulator to run on, such as ANDROID_SERIAL=emulator-5554}"
export ANDROID_SERIAL

cd "$(dirname "$0")"
adb="${ANDROID_HOME:+$ANDROID_HOME/platform-tools/}adb"
target="$(cd .. && pwd)/target/debug"
results=app/build/outputs/androidTest-results/connected/debug
logcat=app/build/outputs/end-to-end/logcat.txt

./gradlew buildServer

# test-redis prints its Redis URL once Redis answers, and removes its
# container when its input closes. Its input is this shell's pipe to the
# coprocess, which closes when this shell exits, however it exits, and
# which bash keeps from every command the script runs.
coproc redis_holder { "$target/test-redis"; }
read -r redis <&"${redis_holder[0]}"

work="$(mktemp -d)"
pids=()
ports=()
# Each run keeps its keys under a prefix of its own, so a run never finds
# the people and consents of the run before it.
prefix="end-to-end-$(date +%s)-$$"
finish() {
    status=$?
    for port in "${ports[@]}"; do
        "$adb" reverse --remove "tcp:$port" || true
    done
    # Each server stops on SIGTERM, and is killed if it has not stopped in
    # five seconds, as when a connection from the emulator stays open.
    for pid in "${pids[@]}"; do
        kill "$pid" 2>/dev/null || true
    done
    for _ in $(seq 50); do
        kill -0 "${pids[@]}" 2>/dev/null || break
        sleep 0.1
    done
    kill -9 "${pids[@]}" 2>/dev/null || true
    if [ "$status" -ne 0 ] && [ -f "$work/transom.log" ]; then
        echo "transom's log:"
        cat "$work/transom.log"
    fi
    # Gradle names only the report of a failed test, which a CI runner
    # throws away, so the failure itself goes to the output.
    for report in "$results"/*.xml; do
        if [ "$status" -ne 0 ] && [ -f "$report" ]; then
            sed -n '/<failure>/,/<\/failure>/p' "$report"
        fi
    done
    # The emulator's whole log is kept for CI to upload, and the output
    # gets the lines of the app's processes, which the test runs in too,
    # and what the system says about the app's processes and notifications.
    # The events buffer holds the notification service's record of each
    # notification the app posts and cancels, and of each cancel it
    # refuses.
    if [ "$status" -ne 0 ]; then
        mkdir -p "$(dirname "$logcat")"
        "$adb" logcat -d -b default -b events -v threadtime >"$logcat" || true
        echo "the app's logcat, from the whole log in $logcat:"
        awk '
            /Start proc [0-9]+:sh\.transom/ {
                match($0, /Start proc [0-9]+/)
                app[substr($0, RSTART + 11, RLENGTH - 11)] = 1
            }
            ($3 in app) || /(ActivityManager|NotificationService|notification_[a-z_]+).*sh\.transom/
        ' "$logcat"
    fi
    rm -rf "$work"
}
trap finish EXIT

# Waits up to ten seconds for a line of the file $1 to match the regular
# expression $2, and prints the first match's group.
awaitLine() {
    for _ in $(seq 100); do
        while IFS= read -r line; do
            if [[ $line =~ $2 ]]; then
                echo "${BASH_REMATCH[1]}"
                return
            fi
        done 2>/dev/null <"$1"
        sleep 0.1
    done
    echo "nothing in $1 matched $2 in ten seconds:" >&2
    cat "$1" >&2
    return 1
}

# The issuer prints its URL once it is listening, and nothing before it.
"$target/test-issuer" \
    --audience transom-tests \
    --client-id transom \
    --client-secret transom-test-secret \
    >"$work/issuer.out" &
pids+=($!)
issuer="$(awaitLine "$work/issuer.out" '^(http://.+)$')"

# transom logs its address once it is listening. None of this shell's own
# TRANSOM_ settings reach it, so a developer's settings cannot change it.
(
    for name in $(compgen -e | grep '^TRANSOM_'); do
        unset "$name"
    done
    export \
        TRANSOM_LISTEN=127.0.0.1:0 \
        TRANSOM_REDIS_URL="$redis" \
        TRANSOM_REDIS_PREFIX="$prefix" \
        TRANSOM_ISSUERS__TEST__URL="$issuer" \
        TRANSOM_ISSUERS__TEST__AUDIENCE=transom-tests \
        TRANSOM_ISSUERS__TEST__NAME_CLAIM=preferred_username \
        TRANSOM_ISSUERS__TEST__CLIENT_ID=transom \
        TRANSOM_ISSUERS__TEST__CLIENT_SECRET=transom-test-secret \
        TRANSOM_ISSUERS__TEST__LABEL="Test issuer" \
        RUST_LOG=info \
        NO_COLOR=1
    exec "$target/transom" serve 2>"$work/transom.log"
) &
pids+=($!)
server="http://$(awaitLine "$work/transom.log" 'listening address=([^ ]+)')"

# transom connects to Redis only when a request needs it, and its health
# check answers whether it can.
if ! curl --silent --fail --max-time 10 "$server/healthz" >/dev/null; then
    echo "transom cannot reach the Redis at $redis" >&2
    exit 1
fi

for url in "$issuer" "$server"; do
    port="${url##*:}"
    "$adb" reverse "tcp:$port" "tcp:$port" >/dev/null
    ports+=("$port")
done

# Chrome opens the Custom Tab, and on a fresh emulator it would first show
# its welcome screens. An emulator's system image is a userdebug build,
# whose Chrome reads its command line from this file.
"$adb" shell 'echo "_ --disable-fre --no-default-browser-check --no-first-run" > /data/local/tmp/chrome-command-line'
"$adb" shell am force-stop com.android.chrome

# The test starts at the sign-in screen, so no earlier install's session
# may remain.
"$adb" uninstall sh.transom >/dev/null 2>&1 || true

rm -rf "$results" "$logcat"
# The log the emulator keeps begins here, so a failure's log is this run's.
"$adb" logcat -b all -c
./gradlew connectedDebugAndroidTest \
    -Pandroid.testInstrumentationRunnerArguments.server="$server" \
    -Pandroid.testInstrumentationRunnerArguments.issuer="$issuer"
