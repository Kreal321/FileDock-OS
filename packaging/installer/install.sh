#!/usr/bin/env bash
# FileDock appliance installer (FR-001 to FR-006, FR-020, FR-058).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
VERSION="${FD_VERSION:-$(cat "$SCRIPT_DIR/RELEASE_VERSION" 2>/dev/null || echo 0.1.0)}"
LOCK="${FD_VERSIONS_LOCK:-$SCRIPT_DIR/versions.lock}"
[[ -f "$LOCK" ]] || LOCK="$ROOT/packaging/versions.lock"
YES=0
CHECK_ONLY=0
OFFLINE_BUNDLE=""
RELEASE="$VERSION"
RELEASE_EXPLICIT=0
ONLINE_BUNDLE=""
trap '[[ -z "$ONLINE_BUNDLE" ]] || rm -rf "$ONLINE_BUNDLE"' EXIT

usage() {
  cat <<'EOF'
Usage: install.sh [--release <version>] [--offline-bundle <path>] [--yes] [--check-only]
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --release) RELEASE="${2:-}"; RELEASE_EXPLICIT=1; shift 2 ;;
    --offline-bundle) OFFLINE_BUNDLE="${2:-}"; shift 2 ;;
    --yes) YES=1; shift ;;
    --check-only) CHECK_ONLY=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unsupported argument: $1" >&2; exit 4 ;;
  esac
done
if [[ -n "$OFFLINE_BUNDLE" && -z "${FD_VERSIONS_LOCK:-}" ]]; then
  LOCK="$OFFLINE_BUNDLE/versions.lock"
fi
if [[ -n "$OFFLINE_BUNDLE" && -z "${FD_VERSION:-}" && -f "$OFFLINE_BUNDLE/RELEASE_VERSION" ]]; then
  VERSION="$(cat "$OFFLINE_BUNDLE/RELEASE_VERSION")"
  if [[ "$RELEASE_EXPLICIT" -eq 0 ]]; then RELEASE="$VERSION"; fi
fi

# --- observable host facts (overridable for tests) ---
os_id() { echo "${FD_TEST_OS_ID:-$(. /etc/os-release 2>/dev/null; echo "${ID:-}")}"; }
os_version() { echo "${FD_TEST_OS_VERSION:-$(. /etc/os-release 2>/dev/null; echo "${VERSION_ID:-}")}"; }
arch() { echo "${FD_TEST_ARCH:-$(uname -m)}"; }
mem_kb() { echo "${FD_TEST_MEM_KB:-$(awk '/MemTotal/ {print $2}' /proc/meminfo 2>/dev/null || echo 0)}"; }
disk_gb() {
  if [[ -n "${FD_TEST_DISK_GB:-}" ]]; then
    echo "$FD_TEST_DISK_GB"
    return
  fi
  df -B1G / 2>/dev/null | awk 'NR==2 {print $2}'
}
has_network() {
  if [[ -n "${FD_TEST_NETWORK:-}" ]]; then
    [[ "$FD_TEST_NETWORK" == "1" ]]
    return
  fi
  ping -c 1 -W 2 1.1.1.1 >/dev/null 2>&1
}
port_in_use() {
  local port="$1"
  local override="FD_TEST_PORT_${port}"
  if [[ -n "${!override:-}" ]]; then
    [[ "${!override}" == "1" ]]
    return
  fi
  if [[ -f /etc/filedock/.installed ]] && systemctl is-active --quiet filedock-management.service; then return 1; fi
  ss -lnt 2>/dev/null | grep -q ":${port} " || netstat -lnt 2>/dev/null | grep -q ":${port} "
}
conflicting_samba() {
  [[ "${FD_TEST_CONFLICT_SAMBA:-0}" == "1" ]] && return 0
  [[ -f /etc/filedock/.installed ]] && return 1
  [[ -f /etc/samba/smb.conf ]] && ! grep -q "FileDock" /etc/samba/smb.conf 2>/dev/null
}
conflicting_nfs() {
  [[ "${FD_TEST_CONFLICT_NFS:-0}" == "1" ]] && return 0
  [[ -f /etc/filedock/.installed ]] && return 1
  [[ -f /etc/exports ]] && [[ -s /etc/exports ]] && ! grep -q "FileDock" /etc/exports 2>/dev/null
}
conflicting_docker() {
  [[ "${FD_TEST_CONFLICT_DOCKER:-0}" == "1" ]] && return 0
  if command -v docker >/dev/null 2>&1 && [[ ! -f /etc/filedock/.docker-owned ]]; then
    docker info >/dev/null 2>&1
  else
    return 1
  fi
}

fail=0
preflight() {
  fail=0
  check() {
    local ok="$1" msg="$2" hint="$3"
    if [[ "$ok" != "1" ]]; then
      echo "FAIL  $msg — $hint"
      fail=1
    fi
  }
  local id ver machine mem disk
  id="$(os_id)"
  ver="$(os_version)"
  machine="$(arch)"
  mem="$(mem_kb)"
  disk="$(disk_gb)"
  check "$([[ "$id" == "debian" && "$ver" == "13" ]] && echo 1 || echo 0)" \
    "Debian 13 is required (found ${id:-unknown} ${ver:-?})" \
    "Install Debian 13 (trixie) amd64."
  check "$([[ "$machine" == "x86_64" || "$machine" == "amd64" ]] && echo 1 || echo 0)" \
    "amd64 architecture is required (found $machine)" \
    "Use an amd64 machine or VM."
  check "$([[ "${mem:-0}" -ge 2097152 ]] && echo 1 || echo 0)" \
    "At least 2 GB RAM is required (found $((${mem:-0} / 1024)) MB)" \
    "Add memory and re-run."
  check "$([[ "${disk:-0}" -ge 16 ]] && echo 1 || echo 0)" \
    "At least 16 GB system disk is required (found ${disk:-0} GB)" \
    "Enlarge the system disk."
  if [[ -z "$OFFLINE_BUNDLE" ]]; then
    check "$(has_network && echo 1 || echo 0)" \
      "Network is required unless --offline-bundle is set" \
      "Connect the appliance or pass --offline-bundle."
  fi
  check "$(conflicting_samba && echo 0 || echo 1)" \
    "Existing Samba configuration is not owned by FileDock" \
    "Move /etc/samba/smb.conf aside or let FileDock own it."
  check "$(conflicting_nfs && echo 0 || echo 1)" \
    "Existing NFS exports are not owned by FileDock" \
    "Clear /etc/exports or mark it as FileDock-owned."
  check "$(conflicting_docker && echo 0 || echo 1)" \
    "Docker from another source is already running" \
    "Remove the foreign Docker install or pass an offline bundle on a clean host."
  check "$(port_in_use 8021 && echo 0 || echo 1)" \
    "TCP port 8021 is already in use" \
    "Stop the service using 8021, then re-run."
  if [[ "$fail" -eq 1 ]]; then
    return 2
  fi
  return 0
}

if ! preflight; then
  exit 2
fi
if [[ "$CHECK_ONLY" -eq 1 ]]; then
  echo "Preflight passed."
  exit 0
fi

# --- install / repair ---
MARKER="/etc/filedock/.installed"
REPAIR=0
if [[ -f "$MARKER" ]]; then
  REPAIR=1
fi

ensure_paths() {
  # FR-005. Repair never touches /var/lib/filedock*, /var/backups/filedock, or /etc/filedock desired state.
  local dirs=(
    /run/filedock
    /usr/share/filedock/subsystems
    /var/cache/filedock
    /mnt/filedock/pools
    /storage
  )
  if [[ "$REPAIR" -eq 0 ]]; then
    dirs+=(
      /var/lib/filedock
      /var/lib/filedock/keys
      /var/lib/filedock/system-pool
      /var/lib/filedock-core
      /var/lib/filedock-management
      /var/backups/filedock
      /var/backups/filedock/appliance-state
      /etc/filedock
      /etc/filedock/samba
    )
  fi
  for dir in "${dirs[@]}"; do
    mkdir -p "$dir"
  done
}

ensure_users() {
  if command -v groupadd >/dev/null 2>&1; then
    getent group filedock-mgmt >/dev/null || groupadd --system filedock-mgmt
    getent group filedock-share >/dev/null || groupadd --system filedock-share
    getent passwd filedock-mgmt >/dev/null || useradd --system --no-create-home --gid filedock-mgmt --shell /usr/sbin/nologin filedock-mgmt
  fi
}

install_journald_cap() {
  mkdir -p /etc/systemd/journald.conf.d
  cat >/etc/systemd/journald.conf.d/filedock.conf <<'EOF'
[Journal]
SystemMaxUse=1G
EOF
}

verify_deb() {
  local name="$1" file="$2"
  local expected
  expected="$(awk -v n="$name" '$1=="deb" && $2==n {print $5; exit}' "$LOCK")"
  [[ -n "$expected" ]] || return 0
  local actual
  actual="$(sha256sum "$file" | awk '{print $1}')"
  if [[ "$actual" != "$expected" ]]; then
    echo "SHA-256 mismatch for $name" >&2
    return 1
  fi
}

configure_repositories() {
  mkdir -p /etc/apt/keyrings /etc/apt/preferences.d
  while read -r kind name url suite component key_url fingerprint; do
    [[ "$kind" == repo ]] || continue
    local key
    key="$(mktemp)"
    curl -fsSL "$key_url" -o "$key"
    local actual
    actual="$(gpg --show-keys --with-colons "$key" | awk -F: '$1=="fpr" {print $10; exit}')"
    [[ "$actual" == "$fingerprint" ]] || { rm -f "$key"; echo "Repository key fingerprint mismatch for $name" >&2; return 1; }
    gpg --batch --yes --dearmor -o "/etc/apt/keyrings/filedock-$name.gpg" "$key"
    rm -f "$key"
    echo "deb [arch=amd64 signed-by=/etc/apt/keyrings/filedock-$name.gpg] $url $suite $component" >"/etc/apt/sources.list.d/filedock-$name.list"
  done <"$LOCK"
  awk '$1=="apt" || $1 ~ /^apt@/ { printf "Package: %s\nPin: version %s\nPin-Priority: 1001\n\n", $2, $3 }' "$LOCK" >/etc/apt/preferences.d/filedock
  apt-get update
}

install_packages() {
  [[ -f "$LOCK" ]] || { echo "Missing versions.lock: $LOCK" >&2; return 1; }
  local bundle="$OFFLINE_BUNDLE"
  if [[ -z "$bundle" ]]; then
    apt-get update
    DEBIAN_FRONTEND=noninteractive apt-get install -y curl ca-certificates gnupg
    configure_repositories || return 1
    bundle="$(mktemp -d)"
    ONLINE_BUNDLE="$bundle"
    local base="${FD_RELEASE_BASE_URL:-https://get.filedock.dev/$RELEASE}"
    for name in filedock-core filedock-management filedock-builtins; do
      curl -fsSL "$base/${name}_${RELEASE}_amd64.deb" -o "$bundle/${name}_${RELEASE}_amd64.deb" || return 1
    done
    curl -fsSL "$base/SHA256SUMS" -o "$bundle/SHA256SUMS" || return 1
    (cd "$bundle" && sha256sum -c SHA256SUMS) || return 1
    while read -r kind name version url checksum; do
      [[ "$kind" == deb ]] || continue
      curl -fsSL "$url" -o "$bundle/${name}_${version}_amd64.deb" || return 1
      verify_deb "$name" "$bundle/${name}_${version}_amd64.deb" || return 1
    done <"$LOCK"
  fi
  [[ -d "$bundle" ]] || { echo "Bundle directory missing: $bundle" >&2; return 1; }
  for name in filedock-core filedock-management filedock-builtins; do
    compgen -G "$bundle/${name}_${RELEASE}_amd64.deb" >/dev/null || { echo "Bundle missing $name $RELEASE" >&2; return 1; }
  done
  for name in rclone kopia; do
    local file
    file="$(find "$bundle" -maxdepth 1 -name "${name}*.deb" -print -quit)"
    [[ -n "$file" ]] && verify_deb "$name" "$file" || { echo "Missing or invalid $name package in bundle" >&2; return 1; }
  done
  if [[ -f "$bundle/SHA256SUMS" ]]; then
    (cd "$bundle" && sha256sum -c SHA256SUMS) || return 1
  fi
  install_bundle_packages "$bundle" || return 1
}

install_bundle_packages() (
  local bundle="$1"
  # Match the bundle solver: recommended packages are not included offline.
  local args=(--no-install-recommends)
  local apt_state=""
  trap '[[ -z "$apt_state" ]] || rm -rf "$apt_state"' EXIT
  if [[ -n "$OFFLINE_BUNDLE" ]]; then
    # --no-download does not restrict dependency candidate selection: repository
    # metadata can still select archives unavailable locally. Resolve using only
    # the supplied files and installed packages, with no repository indexes.
    apt_state="$(mktemp -d)"
    mkdir -p "$apt_state/lists/partial" "$apt_state/sources"
    args+=(
      -o "Dir::Etc::sourcelist=/dev/null"
      -o "Dir::Etc::sourceparts=$apt_state/sources"
      -o "Dir::State::lists=$apt_state/lists"
      -o "Dir::Cache::pkgcache="
      -o "Dir::Cache::srcpkgcache="
    )
  fi
  DEBIAN_FRONTEND=noninteractive apt-get install -y "${args[@]}" "$bundle"/*.deb || return 1
  if [[ "$REPAIR" -eq 1 ]]; then
    # Testing releases can share a package version. Replace the actual FileDock
    # files rather than letting APT skip an already-installed version.
    local filedock_packages=()
    for name in filedock-core filedock-management filedock-builtins; do
      filedock_packages+=("$bundle/${name}_${RELEASE}_amd64.deb")
    done
    DEBIAN_FRONTEND=noninteractive apt-get install -y --reinstall "${args[@]}" "${filedock_packages[@]}" || return 1
  fi
  local pinned
  mapfile -t pinned < <(awk '$1=="apt" || $1 ~ /^apt@/ {print $2 "=" $3}' "$LOCK")
  DEBIAN_FRONTEND=noninteractive apt-get install -y "${args[@]}" "${pinned[@]}" || return 1
)

configure_services() {
  chown root:filedock-mgmt /run/filedock
  chmod 0770 /run/filedock
  chown filedock-mgmt:filedock-mgmt /var/lib/filedock-management
  chmod 0700 /var/lib/filedock-management
  if [[ ! -f /etc/filedock/management.env ]]; then
    cat >/etc/filedock/management.env <<'EOF'
Database__Path=/var/lib/filedock-management/app.db
AdminSocket__Path=/run/filedock/management-admin.sock
Kestrel__HttpPort=8021
Core__Socket=/run/filedock/core.sock
EOF
    chmod 0640 /etc/filedock/management.env
    chown root:filedock-mgmt /etc/filedock/management.env
  fi
  touch /etc/filedock/.docker-owned
  systemctl daemon-reload
  systemd-tmpfiles --create /usr/lib/tmpfiles.d/filedock.conf
  systemctl enable --now filedock-core.socket filedock-core.service filedock-management.service
  systemctl enable --now smbd.service nfs-server.service docker.service
  systemctl restart filedock-core.service filedock-management.service
  for attempt in $(seq 1 60); do
    if curl --silent --fail --unix-socket /run/filedock/management-admin.sock http://localhost/health >/dev/null; then return 0; fi
    sleep 1
  done
  echo "Management service did not become healthy; inspect journalctl -u filedock-management -u filedock-core" >&2
  return 1
}

ui_address() {
  echo "${FD_UI_ADDRESS:-$(hostname -I 2>/dev/null | awk '{print $1}')}"
}

print_setup_code() {
  local code=""
  local output
  if output="$(filedockctl --json setup-code 2>&1)"; then
    code="$(printf '%s' "$output" | sed -n 's/.*"code":"\([^"]*\)".*/\1/p')"
    [[ -n "$code" ]] || { echo "Could not retrieve the setup code" >&2; return 1; }
  elif [[ "$REPAIR" -eq 1 && "$output" == *"already complete"* ]]; then
    echo "FileDock ${RELEASE} repaired. Setup is already complete."
    return 0
  else
    echo "Setup-code request failed: $output" >&2
    return 1
  fi
  local addr
  addr="$(ui_address)"
  [[ -n "$addr" ]] || addr="127.0.0.1"
  echo "FileDock ${RELEASE} installed."
  echo "Open http://${addr}:8021/ to finish setup."
  if [[ -n "$code" ]]; then
    echo "One-time setup code: $code"
  fi
}

[[ "$(id -u)" -eq 0 ]] || { echo "Run the installer as root." >&2; exit 3; }
ensure_users
ensure_paths
install_journald_cap
install_packages || exit 3
configure_services || exit 3
print_setup_code || exit 3
mkdir -p /etc/filedock
if [[ -f "$SCRIPT_DIR/bootstrap.sh" ]]; then
  install -D -m 0755 "$SCRIPT_DIR/bootstrap.sh" /usr/local/sbin/filedock-bootstrap
fi
date -u +"%Y-%m-%dT%H:%M:%SZ" >"$MARKER"
printf '%s\n' "$RELEASE" > /etc/filedock/RELEASE_VERSION
build_file="${OFFLINE_BUNDLE:-$SCRIPT_DIR}/BUILD.txt"
if [[ -f "$build_file" ]]; then
  install -m 0644 "$build_file" /etc/filedock/BUILD.txt
else
  rm -f /etc/filedock/BUILD.txt /etc/filedock/release.json
fi
exit 0
