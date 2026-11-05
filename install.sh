#!/bin/sh
set -eu

repository=https://github.com/Ownistic/vader3-pro-linux-driver
asset=vader3p-steam-linux-x86_64
replace=false

case ${1-} in
    '') ;;
    --replace) replace=true; shift ;;
    *) printf 'Usage: sh install.sh [--replace]\n' >&2; exit 1 ;;
esac
if [ "$#" -ne 0 ]; then
    printf 'Usage: sh install.sh [--replace]\n' >&2
    exit 1
fi
if [ "$(id -u)" -eq 0 ]; then
    printf 'Run this installer as your normal user, not root.\n' >&2
    exit 1
fi
if [ "$(uname -s)" != Linux ] || [ "$(uname -m)" != x86_64 ]; then
    printf 'The published binary requires x86_64 Linux. Build from source on other systems.\n' >&2
    exit 1
fi
for command in curl sha256sum mktemp install; do
    if ! command -v "$command" >/dev/null 2>&1; then
        printf 'Missing required command: %s\n' "$command" >&2
        exit 1
    fi
done
: "${HOME:?HOME must be set}"
destination=$HOME/.local/bin/vader3p-steam
if [ -L "$destination" ] || [ -d "$destination" ]; then
    printf 'Refusing to replace a symlink or directory: %s\n' "$destination" >&2
    exit 1
fi
if [ -e "$destination" ] && [ ! -f "$destination" ]; then
    printf 'Refusing to replace a non-regular file: %s\n' "$destination" >&2
    exit 1
fi
if [ -e "$destination" ] && [ "$replace" != true ]; then
    printf 'An executable already exists at %s. Back it up and review the migration guide before using --replace.\n' "$destination" >&2
    exit 1
fi

tmp=$(mktemp -d "${TMPDIR:-/tmp}/vader3p-install.XXXXXXXX")
staged=
cleanup() {
    if [ -n "$staged" ]; then rm -f -- "$staged"; fi
    rm -rf -- "$tmp"
}
trap cleanup EXIT
trap 'exit 1' HUP INT TERM

base=$repository/releases/latest/download
printf 'Downloading the latest release...\n'
curl --fail --location --silent --show-error --proto '=https' --proto-redir '=https' \
    "$base/$asset" --output "$tmp/$asset"
curl --fail --location --silent --show-error --proto '=https' --proto-redir '=https' \
    "$base/$asset.sha256" --output "$tmp/$asset.sha256"
(
    cd "$tmp"
    # Reject manifests that could name another path or contain extra entries.
    expected=$(sha256sum "$asset")
    actual=$(cat "$asset.sha256")
    [ "$actual" = "$expected" ] || { printf 'Checksum verification failed.\n' >&2; exit 1; }
)

mkdir -p "$HOME/.local/bin"
staged=$(mktemp "$HOME/.local/bin/.vader3p-steam.XXXXXXXX")
install -m 755 "$tmp/$asset" "$staged"
if [ -L "$destination" ] || [ -d "$destination" ]; then
    printf 'Destination changed during installation; refusing to replace it.\n' >&2
    exit 1
fi
if [ -e "$destination" ] && [ ! -f "$destination" ]; then
    printf 'Destination is not a regular file; refusing to replace it.\n' >&2
    exit 1
fi
if [ "$replace" = true ]; then
    mv -fT -- "$staged" "$destination"
else
    # A hard link provides an atomic no-clobber install, even if another process
    # creates the destination after the initial check.
    ln -- "$staged" "$destination" || { printf 'Destination already exists; nothing was replaced.\n' >&2; exit 1; }
    rm -f -- "$staged"
fi
staged=
printf 'Installed %s\n' "$destination"
printf 'Configuration and services were not changed. See %s#device-access before running.\n' "$repository"
