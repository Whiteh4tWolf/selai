#!/usr/bin/env bash
# Runs a command in a private D-Bus session with an unlocked GNOME login keyring,
# so the Linux Secret Service backend (used by selai-secrets) can be exercised
# without a desktop. Usage: scripts/ci/with-keyring.sh <command> [args...]
set -euo pipefail

command -v dbus-run-session >/dev/null || { echo "dbus-run-session is required" >&2; exit 1; }
command -v gnome-keyring-daemon >/dev/null || { echo "gnome-keyring-daemon is required" >&2; exit 1; }

# Keep the toolchain reachable even though HOME is replaced below.
export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
export RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"
TMP_HOME="$(mktemp -d)"
export HOME="$TMP_HOME" XDG_DATA_HOME="$TMP_HOME/data" XDG_CONFIG_HOME="$TMP_HOME/cfg"

exec dbus-run-session -- bash -c '
  # Creating the login keyring needs a non-empty password; it is throwaway.
  eval "$(printf "selai-ci-keyring" | gnome-keyring-daemon --unlock --components=secrets 2>/dev/null)"
  export GNOME_KEYRING_CONTROL DBUS_SESSION_BUS_ADDRESS
  "$@"
' bash "$@"
