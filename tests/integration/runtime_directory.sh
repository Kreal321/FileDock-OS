#!/usr/bin/env bash
# Root-only Debian regression; creates accounts/directories only under a temporary root.
set -euo pipefail
[[ "$EUID" -eq 0 ]] || { echo 'Run as root on Debian.' >&2; exit 2; }
repo="$(cd "$(dirname "$0")/../.." && pwd)"
testroot="$(mktemp -d /var/tmp/filedock-runtime-test.XXXXXXXX)"
trap 'rm -rf -- "$testroot"' EXIT
install -d "$testroot/etc" "$testroot/usr/lib/sysusers.d" "$testroot/usr/lib/tmpfiles.d"
printf 'root:x:0:0:root:/root:/bin/sh\n' > "$testroot/etc/passwd"
printf 'root:x:0:\n' > "$testroot/etc/group"
cp "$repo/packaging/systemd/filedock-sysusers.conf" "$testroot/usr/lib/sysusers.d/filedock.conf"
cp "$repo/packaging/systemd/filedock.conf" "$testroot/usr/lib/tmpfiles.d/filedock.conf"
systemd-sysusers --root="$testroot"
gid="$(awk -F: '$1=="filedock-mgmt" {print $3}' "$testroot/etc/group")"
[[ -n "$gid" ]]
check_directory() {
  [[ "$(stat -c '%a:%u:%g' "$testroot/run/filedock")" == "770:0:$gid" ]]
  [[ "$(stat -c '%a:%u:%g' "$testroot/run/filedock/secrets")" == '700:0:0' ]]
}
systemd-tmpfiles --root="$testroot" --create
check_directory
echo 'PASS: fresh account and runtime hierarchy'
chown root:root "$testroot/run/filedock"
chmod 0755 "$testroot/run/filedock"
systemd-tmpfiles --root="$testroot" --create
check_directory
echo 'PASS: wrong ownership and permissions repaired'
rm -rf -- "$testroot/run"
systemd-tmpfiles --root="$testroot" --create
check_directory
echo 'PASS: empty runtime filesystem recreated'
