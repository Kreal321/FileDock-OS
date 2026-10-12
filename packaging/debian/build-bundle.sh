#!/usr/bin/env bash
# Complete offline bundle for a fresh Debian 13 amd64 target. Run after make packages.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
DIST="$(realpath "${1:-$ROOT/dist}")"
LOCK="$DIST/versions.lock"
[[ "$(id -u)" == 0 ]] || { echo "Run with sudo on the Debian 13 build VM." >&2; exit 1; }
. /etc/os-release
[[ "$ID" == debian && "$VERSION_ID" == 13 && "$(uname -m)" == x86_64 ]] || { echo "Debian 13 amd64 is required." >&2; exit 1; }
[[ -f "$DIST/install.sh" && -f "$LOCK" ]] || { echo "Run make packages first." >&2; exit 1; }
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
cd "$DIST"
while read -r kind name version url checksum; do
  [[ "$kind" == deb ]] || continue
  filename="${name}_${version}_amd64.deb"
  curl -fsSL "$url" -o "$filename"
  printf '%s  %s\n' "$checksum" "$filename" | sha256sum -c -
done <"$LOCK"
# Download the dependency closure even when packages are installed on the build host.
mapfile -t direct < <(awk '$1=="apt" || $1 ~ /^apt@/ {print $2}' "$LOCK")
# Include dependencies declared by upstream/local .debs as well as the locked roots.
mapfile -t deb_roots < <(for package in ./*.deb; do for field in Depends Pre-Depends; do dpkg-deb -f "$package" "$field"; done; done \
  | tr ',|' '\n' | sed -E 's/\([^)]*\)//g; s/^[[:space:]]+//; s/[[:space:]]+$//; s/:(any|native)$//' \
  | awk 'NF && $0 !~ /^filedock-/ {print}' | sort -u)
closure_file="$(mktemp)"
apt-cache depends --recurse --important --no-recommends --no-suggests --no-conflicts --no-breaks --no-replaces --no-enhances \
  "${direct[@]}" "${deb_roots[@]}" libicu76 libssl3t64 libstdc++6 zlib1g curl ca-certificates gnupg openssl >"$closure_file"
mapfile -t closure < <(awk '/^[a-zA-Z0-9][^ ]*$/ && !/</' "$closure_file" | sort -u)
rm -f "$closure_file"
for package in "${closure[@]}"; do
  version="$(awk -v p="$package" '($1=="apt" || $1 ~ /^apt@/) && $2==p {print $3; exit}' "$LOCK")"
  if [[ -n "$version" ]]; then apt-get download "$package=$version"; else apt-get download "$package"; fi
done
sha256sum ./*.deb >SHA256SUMS
printf 'Offline bundle ready: %s\n' "$DIST"
