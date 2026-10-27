#!/usr/bin/env bash
# One-shot verification run for the Vader 3 Pro translator.
#
# Safe by construction: no firmware writes, no vendor/OTA characteristic writes,
# no 0x13 and no 0x33. The only device traffic is reading hidraw plus, if rumble
# is enabled, the proven 4-byte 0x11 rumble frame.
#
# Usage:
#   cd driver && ./contrib/test-run.sh
#
# Writes a transcript to /tmp/opencode/vader3p-test.log and prints its path.

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LOG=/tmp/opencode/vader3p-test.log
CFG=/tmp/opencode/vader3p-test.toml
mkdir -p /tmp/opencode
: > "$LOG"

say() { printf '\n=== %s ===\n' "$*" 2>&1 | tee -a "$LOG"; }
run() {
    printf '\n$ %s\n' "$*" 2>&1 | tee -a "$LOG"
    "$@" 2>&1 | tee -a "$LOG"
    return "${PIPESTATUS[0]}"
}

say "1. Build"
run cargo build --release --manifest-path "$ROOT/Cargo.toml"

say "2. Unit and integration tests"
run cargo test --release --manifest-path "$ROOT/Cargo.toml"

say "3. Virtual pad signal routing (automated: no controller, no root, no key presses)"
run cargo run --quiet --release --manifest-path "$ROOT/Cargo.toml" --example signal_probe

say "4. Locate the controller"
HIDRAW=""
for uevent in /sys/class/hidraw/hidraw*/device/uevent; do
    if grep -qi vader "$uevent" 2>/dev/null; then
        HIDRAW="/dev/$(basename "$(dirname "$(dirname "$uevent")")")"
        break
    fi
done

if [ -z "$HIDRAW" ]; then
    echo "controller not found on hidraw." | tee -a "$LOG"
    echo "Connect it in Bluetooth DInput mode and run this script again." | tee -a "$LOG"
else
    echo "hidraw node: $HIDRAW" | tee -a "$LOG"
    ADDR=$(bluetoothctl devices 2>/dev/null | grep -i vader | awk '{print $2}' | head -1)
    echo "bluetooth address: ${ADDR:-none found}" | tee -a "$LOG"

    {
        echo 'log_level = "debug"'
        echo 'rumble = true'
        echo 'suppress_physical = true'
        if [ -n "${ADDR:-}" ]; then
            echo "bluetooth_address = \"$ADDR\""
        fi
    } > "$CFG"

    say "5. Generated config"
    cat "$CFG" | tee -a "$LOG"

    say "6. Does the daemon start and see the controller?"
    timeout 6 "$ROOT/target/release/vader3p-steam" -c "$CFG" 2>&1 | tee -a "$LOG"
    echo "(exit $? — 124 means it ran until the timeout, which is success)" | tee -a "$LOG"
fi

say "done"
printf 'transcript: %s\n' "$LOG" | tee -a "$LOG"
