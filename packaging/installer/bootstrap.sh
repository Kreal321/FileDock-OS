#!/usr/bin/env bash
# Small online entry point; the verified bundle's install.sh owns installation.
set -euo pipefail
REPOSITORY='Kreal321/FileDock-OS'
ACTION='menu'
STABLE=0
YES=0
usage() {
  cat <<'HELP'
Usage: filedock-bootstrap [--install | --update | --repair | --check] [--stable] [--yes]
Without an action, show the install/update menu. Latest includes testing prereleases.
--stable selects only stable releases. --yes runs the selected action noninteractively.
HELP
}
for argument in "$@"; do
  case "$argument" in
    --install|--update|--repair|--check)
      [[ "$ACTION" == menu ]] || { echo 'Choose only one action.' >&2; exit 4; }
      ACTION="${argument#--}" ;;
    --stable) STABLE=1 ;;
    --yes) YES=1 ;;
    -h|--help) usage; exit 0 ;;
    *) usage >&2; exit 4 ;;
  esac
done
[[ "$EUID" -eq 0 ]] || { echo 'Run as root on the FileDock VM.' >&2; exit 2; }
. /etc/os-release
[[ "$ID" == debian && "$VERSION_ID" == 13 && "$(uname -m)" == x86_64 ]] || {
  echo 'FileDock requires Debian 13 amd64.' >&2; exit 2;
}
[[ -d /run/systemd/system ]] || { echo 'A running systemd host is required.' >&2; exit 2; }
[[ -f "${BASH_SOURCE[0]}" ]] || { echo 'Download this script to a file, then run it with bash.' >&2; exit 4; }
[[ "$(awk '/MemTotal/ {print $2}' /proc/meminfo)" -ge 2097152 ]] || { echo 'At least 2 GB RAM is required.' >&2; exit 2; }
[[ "$(df -B1G / | awk 'NR==2 {print $2}')" -ge 16 ]] || { echo 'At least 16 GB system disk capacity is required.' >&2; exit 2; }
exec 9>/run/lock/filedock-bootstrap.lock
flock -n 9 || { echo 'Another FileDock bootstrap is running.' >&2; exit 3; }
for tool in curl python3; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    [[ "$ACTION" != check ]] || { echo 'Install curl, ca-certificates and python3 before checking releases.' >&2; exit 2; }
    apt-get update
    DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends curl ca-certificates python3
    break
  fi
done
if [[ ! -s /etc/ssl/certs/ca-certificates.crt ]]; then
  [[ "$ACTION" != check ]] || { echo 'Install ca-certificates before checking releases.' >&2; exit 2; }
  apt-get update
  DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends ca-certificates
fi
WORK="$(mktemp -d /var/tmp/filedock-bootstrap.XXXXXXXX)"
RECORD_TMP=''
cleanup() {
  local result=$?
  trap - EXIT
  rm -rf -- "$WORK"
  [[ -z "$RECORD_TMP" ]] || rm -f -- "$RECORD_TMP"
  exit "$result"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
fetch() {
  curl --fail --show-error --location --proto '=https' --proto-redir '=https' \
    --connect-timeout 15 --max-time 3600 --retry 3 \
    --user-agent 'FileDock-bootstrap' "$1" -o "$2"
}
# Enumerate releases because GitHub's /latest endpoint excludes prereleases.
page=1
while :; do
  fetch "https://api.github.com/repos/$REPOSITORY/releases?per_page=100&page=$page" "$WORK/page-$page.json"
  count="$(python3 -c 'import json,sys; data=json.load(open(sys.argv[1])); assert isinstance(data,list), "Invalid release response"; print(len(data))' "$WORK/page-$page.json")"
  [[ "$count" -eq 100 ]] || break
  page=$((page + 1))
done
python3 - "$WORK" "$REPOSITORY" "$STABLE" <<'PY'
import glob, json, pathlib, re, sys
from urllib.parse import quote
work, repository, stable = pathlib.Path(sys.argv[1]), sys.argv[2], sys.argv[3] == '1'
releases = [release for file in glob.glob(str(work / 'page-*.json')) for release in json.loads(pathlib.Path(file).read_text())]
candidates = []
for release in releases:
    if release.get('draft') or not release.get('published_at') or (stable and release.get('prerelease')):
        continue
    assets = {a['name']: a for a in release.get('assets', []) if a.get('state') == 'uploaded'}
    archives = [name for name in assets if re.fullmatch(r'filedock-offline-[A-Za-z0-9_.-]+-debian13-amd64\.tar\.gz', name) and name + '.sha256' in assets]
    if len(archives) == 1:
        candidates.append((release, assets, archives[0]))
if not candidates:
    raise SystemExit('No complete Debian 13 amd64 release bundle was found for this channel.')
release, assets, archive = max(candidates, key=lambda item: (item[0]['published_at'], item[0]['id']))
tag = release['tag_name']
version = re.fullmatch(r'(?:v|test-)([0-9]+\.[0-9]+\.[0-9]+)(?:-[0-9]+-[0-9]+)?', tag)
if not version:
    raise SystemExit('The selected release tag does not contain a supported package version.')
base = f'https://github.com/{repository}/releases/download/{quote(tag, safe="")}/'
for name in (archive, archive + '.sha256'):
    if assets[name]['browser_download_url'] != base + quote(name, safe=''):
        raise SystemExit('Unexpected release asset URL.')
installed_tag = ''
build = pathlib.Path('/etc/filedock/BUILD.txt')
if build.is_file():
    installed_tag = next((line.removeprefix('Release: ').strip() for line in build.read_text().splitlines() if line.startswith('Release: ')), '')
record = pathlib.Path('/etc/filedock/release.json')
try:
    previous = json.loads(record.read_text()) if record.is_file() else {}
except (OSError, ValueError):
    previous = {}
if not installed_tag:
    installed_tag = previous.get('tag', '')
installed_at = next((r['published_at'] for r in releases if r['tag_name'] == installed_tag), '')
if not installed_at and previous.get('tag') == installed_tag:
    installed_at = previous.get('published_at', '')
metadata = dict(tag=tag, version=version[1], published_at=release['published_at'], release_id=release['id'], prerelease=release['prerelease'], archive=archive,
                archive_url=assets[archive]['browser_download_url'], checksum_url=assets[archive + '.sha256']['browser_download_url'], size=assets[archive]['size'],
                installed_tag=installed_tag, installed_published_at=installed_at)
(work / 'release.json').write_text(json.dumps(metadata))
PY
mapfile -t RELEASE_INFO < <(python3 - "$WORK/release.json" <<'PY'
import json, sys
r = json.load(open(sys.argv[1]))
for key in ('tag', 'version', 'published_at', 'archive', 'archive_url', 'checksum_url', 'installed_tag', 'installed_published_at'):
    print(r[key])
PY
)
[[ "${#RELEASE_INFO[@]}" -eq 8 ]] || { echo 'Invalid release metadata.' >&2; exit 3; }
TAG="${RELEASE_INFO[0]}"
VERSION="${RELEASE_INFO[1]}"
ARCHIVE="${RELEASE_INFO[3]}"
CURRENT_TAG="${RELEASE_INFO[6]}"
INSTALLED=0
ALL_CURRENT=1
OLDER_TARGET=0
printf '\nFileDock — Debian %s amd64\nRepository: https://github.com/%s/releases\n' "$VERSION_ID" "$REPOSITORY"
for package in filedock-core filedock-management filedock-builtins; do
  information="$(dpkg-query -W -f='${Status}|${Version}' "$package" 2>/dev/null || true)"
  if [[ "$information" == 'install ok installed|'* ]]; then
    INSTALLED=1
    current="${information#*|}"
    printf '  %-21s %s\n' "$package" "$current"
    [[ "$current" == "$VERSION" ]] || ALL_CURRENT=0
    if dpkg --compare-versions "$current" gt "$VERSION"; then OLDER_TARGET=1; fi
  else
    printf '  %-21s not installed\n' "$package"
    ALL_CURRENT=0
  fi
done
[[ ! -f /etc/filedock/.installed ]] || INSTALLED=1
printf 'Installed release: %s\nLatest release:    %s (package %s)\n' "${CURRENT_TAG:-unknown}" "$TAG" "$VERSION"
if [[ -n "${RELEASE_INFO[7]}" && "${RELEASE_INFO[7]}" > "${RELEASE_INFO[2]}" ]]; then OLDER_TARGET=1; fi
if [[ "$CURRENT_TAG" == "$TAG" && "$ALL_CURRENT" -eq 1 ]]; then
  echo 'Status: already on the latest release.'
elif [[ "$OLDER_TARGET" -eq 1 ]]; then
  echo 'Status: the installed version is newer than the selected release.'
elif [[ "$INSTALLED" -eq 1 ]]; then
  echo 'Status: an update is available, or the installed release identity is unknown.'
else
  echo 'Status: ready for a new installation.'
fi
if [[ "$ACTION" == menu ]]; then
  [[ -t 0 || -t 1 ]] || { echo 'Choose --install, --update, --repair or --check when running noninteractively.' >&2; exit 4; }
  printf '\n1) Install latest\n2) Update existing installation\n3) Check versions only\n4) Reinstall / repair latest\n0) Exit\nChoice: '
  read -r choice </dev/tty
  case "$choice" in
    1) ACTION=install ;; 2) ACTION=update ;; 3) ACTION=check ;; 4) ACTION=repair ;; 0) exit 0 ;;
    *) echo 'Unknown choice.' >&2; exit 4 ;;
  esac
fi
[[ "$ACTION" != check ]] || exit 0
if [[ "$ACTION" == install && "$INSTALLED" -eq 1 ]]; then echo 'FileDock is already present. Choose Update or Repair.' >&2; exit 4; fi
if [[ "$ACTION" != install && "$INSTALLED" -eq 0 ]]; then echo 'No FileDock installation was found. Choose Install.' >&2; exit 4; fi
[[ "$OLDER_TARGET" -eq 0 ]] || { echo 'Refusing to downgrade the installed release.' >&2; exit 4; }
if [[ "$ACTION" == update && "$CURRENT_TAG" == "$TAG" && "$ALL_CURRENT" -eq 1 ]]; then
  if [[ ! -x /usr/local/sbin/filedock-bootstrap ]]; then
    install -D -m 0755 "${BASH_SOURCE[0]}" /usr/local/sbin/filedock-bootstrap
  fi
  echo 'Next time, run: filedock-bootstrap --update'
  exit 0
fi
if [[ "$YES" -eq 0 ]]; then
  [[ -t 0 || -t 1 ]] || { echo 'Use --yes with an explicit action for noninteractive installation.' >&2; exit 4; }
  printf '\n%s release %s? [y/N] ' "$ACTION" "$TAG"
  read -r answer </dev/tty
  [[ "$answer" == y || "$answer" == Y || "$answer" == yes ]] || exit 0
fi
fetch "${RELEASE_INFO[4]}" "$WORK/$ARCHIVE"
fetch "${RELEASE_INFO[5]}" "$WORK/$ARCHIVE.sha256"
python3 - "$WORK" "$ARCHIVE" <<'PY'
import hashlib, pathlib, re, shutil, sys, tarfile
work, name = pathlib.Path(sys.argv[1]), sys.argv[2]
lines = (work / (name + '.sha256')).read_text().splitlines()
if len(lines) != 1 or not re.fullmatch(r'[a-fA-F0-9]{64} [ *]' + re.escape(name), lines[0]):
    raise SystemExit('Invalid archive checksum file.')
with (work / name).open('rb') as downloaded:
    if hashlib.file_digest(downloaded, 'sha256').hexdigest() != lines[0][:64].lower():
        raise SystemExit('Archive SHA-256 mismatch; nothing will be installed.')
print(name + ': OK')
with tarfile.open(work / name, 'r:gz') as archive:
    members = archive.getmembers()
    for member in members:
        path = pathlib.PurePosixPath(member.name)
        if path.is_absolute() or '..' in path.parts or not (member.isfile() or member.isdir()):
            raise SystemExit('Unsafe archive entry: ' + member.name)
    if sum(m.size for m in members if m.isfile()) >= shutil.disk_usage(work).free:
        raise SystemExit('Not enough temporary disk space to extract the bundle.')
PY
mkdir "$WORK/bundle"
tar --no-same-owner --no-same-permissions -xzf "$WORK/$ARCHIVE" -C "$WORK/bundle"
for file in install.sh RELEASE_VERSION versions.lock SHA256SUMS BUILD.txt; do
  [[ -f "$WORK/bundle/$file" ]] || { echo "Release bundle is missing $file." >&2; exit 3; }
done
[[ "$(cat "$WORK/bundle/RELEASE_VERSION")" == "$VERSION" ]] || { echo 'Package version does not match the selected release.' >&2; exit 3; }
grep -Fxq "Release: $TAG" "$WORK/bundle/BUILD.txt" || { echo 'Bundle release identity mismatch.' >&2; exit 3; }
grep -Fxq "Package version: $VERSION" "$WORK/bundle/BUILD.txt" || { echo 'Bundle build version mismatch.' >&2; exit 3; }
grep -Fxq 'Target: Debian 13 amd64' "$WORK/bundle/BUILD.txt" || { echo 'Bundle target mismatch.' >&2; exit 3; }
(cd "$WORK/bundle" && sha256sum -c SHA256SUMS)
bash "$WORK/bundle/install.sh" --offline-bundle "$WORK/bundle" --check-only
arguments=(--offline-bundle "$WORK/bundle")
[[ "$YES" -eq 0 ]] || arguments+=(--yes)
bash "$WORK/bundle/install.sh" "${arguments[@]}"
# Record provenance only after the bundled installer completes successfully.
mkdir -p /etc/filedock
RECORD_TMP="$(mktemp /etc/filedock/.release.XXXXXXXX)"
python3 - "$WORK/release.json" "$WORK/$ARCHIVE" "$RECORD_TMP" <<'PY'
import datetime, hashlib, json, sys
r = json.load(open(sys.argv[1]))
for key in ('installed_tag', 'installed_published_at', 'archive_url', 'checksum_url', 'size'):
    r.pop(key, None)
with open(sys.argv[2], 'rb') as archive:
    r['archive_sha256'] = hashlib.file_digest(archive, 'sha256').hexdigest()
r['installed_at'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
with open(sys.argv[3], 'w') as output:
    json.dump(r, output, indent=2)
PY
chmod 0644 "$RECORD_TMP"
mv -f "$RECORD_TMP" /etc/filedock/release.json
RECORD_TMP=''
source_script="$WORK/bundle/bootstrap.sh"
[[ -f "$source_script" ]] || source_script="${BASH_SOURCE[0]}"
install -D -m 0755 "$source_script" /usr/local/sbin/filedock-bootstrap
printf '\nFileDock %s installed. Temporary downloads will be removed.\n' "$TAG"
echo 'Next time, run: filedock-bootstrap --update'
