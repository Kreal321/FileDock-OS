#!/usr/bin/env bash
# Complete offline bundle for a fresh Debian 13 amd64 target. Run after make packages.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
DIST="$(realpath "${1:-$ROOT/dist}")"
LOCK="$DIST/versions.lock"
[[ "$(id -u)" == 0 ]] || { echo "Run with sudo on the Debian 13 build VM." >&2; exit 1; }
. /etc/os-release
[[ "$ID" == debian && "$VERSION_ID" == 13 && "$(uname -m)" == x86_64 ]] || { echo "Debian 13 amd64 is required." >&2; exit 1; }
[[ -f "$DIST/install.sh" && -f "$DIST/RELEASE_VERSION" && -f "$LOCK" ]] || { echo "Run make packages first." >&2; exit 1; }
apt-get update
apt-get install -y curl gnupg ca-certificates
mkdir -p /etc/apt/keyrings /etc/apt/preferences.d
while read -r kind name url suite component key_url fingerprint; do
  [[ "$kind" == repo ]] || continue
  key="$(mktemp)"
  curl -fsSL "$key_url" -o "$key"
  actual="$(gpg --show-keys --with-colons "$key" | awk -F: '$1=="fpr" {print $10; exit}')"
  [[ "$actual" == "$fingerprint" ]] || { echo "Repository fingerprint mismatch" >&2; exit 1; }
  gpg --batch --yes --dearmor -o "/etc/apt/keyrings/filedock-$name.gpg" "$key"
  rm "$key"
  echo "deb [arch=amd64 signed-by=/etc/apt/keyrings/filedock-$name.gpg] $url $suite $component" >"/etc/apt/sources.list.d/filedock-$name.list"
done <"$LOCK"
awk '$1=="apt" || $1 ~ /^apt@/ {printf "Package: %s\nPin: version %s\nPin-Priority: 1001\n\n",$2,$3}' "$LOCK" >/etc/apt/preferences.d/filedock
apt-get update
# Resolve against an empty package status so dependencies installed on the CI build
# host are still downloaded. APT selects one compatible provider for alternatives;
# apt-cache's recursive listing includes mutually exclusive providers.
solver_dir="$(mktemp -d)"
trap 'rm -rf "$solver_dir"' EXIT
chmod 0755 "$solver_dir"
mkdir -p "$solver_dir/inputs" "$solver_dir/archives/partial"
: >"$solver_dir/status"
version="$(cat "$DIST/RELEASE_VERSION")"
for name in filedock-core filedock-management filedock-builtins; do
  cp "$DIST/${name}_${version}_amd64.deb" "$solver_dir/inputs/"
done
while read -r kind name version url checksum; do
  [[ "$kind" == deb ]] || continue
  filename="${name}_${version}_amd64.deb"
  curl -fsSL "$url" -o "$solver_dir/inputs/$filename"
  (cd "$solver_dir/inputs" && printf '%s  %s\n' "$checksum" "$filename" | sha256sum -c -)
done <"$LOCK"
mapfile -t pinned < <(awk '$1=="apt" || $1 ~ /^apt@/ {print $2 "=" $3}' "$LOCK")
apt-get install -y --download-only --no-install-recommends \
  -o "Dir::State::status=$solver_dir/status" \
  -o "Dir::State::extended_states=$solver_dir/extended_states" \
  -o "Dir::Cache::archives=$solver_dir/archives" \
  "$solver_dir/inputs/"*.deb "${pinned[@]}" curl ca-certificates gnupg
# Replace dependency artifacts only after resolution and download succeed. This
# also removes conflicting providers left by a previous bundle-generation run.
find "$DIST" -maxdepth 1 -type f -name '*.deb' -delete
cp "$solver_dir/inputs/"*.deb "$solver_dir/archives/"*.deb "$DIST/"
(cd "$DIST" && sha256sum ./*.deb >SHA256SUMS)
printf 'Offline bundle ready: %s\n' "$DIST"
