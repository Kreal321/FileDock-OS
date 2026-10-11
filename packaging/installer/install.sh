#!/usr/bin/env bash
# FileDock appliance installer (FR-001 to FR-006, FR-020, FR-058).
set -euo pipefail

VERSION="${FD_VERSION:-0.1.0}"
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
LOCK="${FD_VERSIONS_LOCK:-$ROOT/packaging/versions.lock}"
YES=0
CHECK_ONLY=0
OFFLINE_BUNDLE=""
RELEASE="$VERSION"

usage() {
  cat <<'EOF'
Usage: install.sh [--release <version>] [--offline-bundle <path>] [--yes] [--check-only]
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --release) RELEASE="${2:-}"; shift 2 ;;
    --offline-bundle) OFFLINE_BUNDLE="${2:-}"; shift 2 ;;
    --yes) YES=1; shift ;;
    --check-only) CHECK_ONLY=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unsupported argument: $1" >&2; exit 4 ;;
  esac
done

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
  if [[ -n "${FD_TEST_PORT_443:-}" ]]; then
    [[ "$FD_TEST_PORT_443" == "1" ]]
    return
  fi
  ss -lnt 2>/dev/null | grep -q ":${port} " || netstat -lnt 2>/dev/null | grep -q ":${port} "
}
conflicting_samba() {
  [[ "${FD_TEST_CONFLICT_SAMBA:-0}" == "1" ]] && return 0
  [[ -f /etc/samba/smb.conf ]] && ! grep -q "FileDock" /etc/samba/smb.conf 2>/dev/null
}
conflicting_nfs() {
  [[ "${FD_TEST_CONFLICT_NFS:-0}" == "1" ]] && return 0
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
  check "$(port_in_use 443 && echo 0 || echo 1)" \
    "TCP port 443 is already in use" \
    "Stop the service using 443, then re-run."
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

install_tls() {
  local dir="/var/lib/filedock-management/tls"
  if [[ "$REPAIR" -eq 1 && -f "$dir/tls.crt" ]]; then
    return 0
  fi
  if [[ "$REPAIR" -eq 1 ]]; then
    mkdir -p "$dir"
  fi
  if [[ "$REPAIR" -eq 0 ]]; then
    mkdir -p "$dir"
  fi
  if command -v openssl >/dev/null 2>&1 && [[ ! -f "$dir/tls.crt" ]]; then
    openssl req -x509 -newkey rsa:2048 -sha256 -days 3650 -nodes \
      -keyout "$dir/tls.key" -out "$dir/tls.crt" \
      -subj "/CN=filedock" >/dev/null 2>&1 || true
    chmod 640 "$dir/tls.key" 2>/dev/null || true
  fi
}

verify_deb() {
  local name="$1" file="$2"
  local expected
  expected="$(awk -v n="$name" '$1=="deb" && $2==n {print $4; exit}' "$LOCK")"
  [[ -n "$expected" ]] || return 0
  local actual
  actual="$(sha256sum "$file" | awk '{print $1}')"
  if [[ "$actual" != "$expected" ]]; then
    echo "SHA-256 mismatch for $name" >&2
    return 1
  fi
}

install_packages() {
  if [[ ! -f /etc/debian_version ]]; then
    echo "Not Debian; skipping apt package install."
    return 0
  fi
  if [[ -n "$OFFLINE_BUNDLE" ]]; then
    local bundle="$OFFLINE_BUNDLE"
    if [[ -d "$bundle" ]]; then
      for deb in rclone kopia; do
        local f
        f="$(find "$bundle" -name "${deb}*.deb" | head -n1 || true)"
        if [[ -n "$f" ]]; then
          verify_deb "$deb" "$f" || return 1
        fi
      done
      if command -v apt-get >/dev/null 2>&1; then
        DEBIAN_FRONTEND=noninteractive apt-get install -y --no-upgrade \
          "$bundle"/filedock-core_*.deb \
          "$bundle"/filedock-management_*.deb \
          "$bundle"/filedock-builtins_*.deb 2>/dev/null || \
          dpkg -i "$bundle"/filedock-*.deb || true
      fi
    fi
    return 0
  fi
  if command -v apt-get >/dev/null 2>&1 && [[ -f "$LOCK" ]]; then
    mkdir -p /etc/apt/preferences.d
    awk '$1=="apt" { printf "Package: %s\nPin: version %s\nPin-Priority: 1001\n\n", $2, $3 }' "$LOCK" \
      >/etc/apt/preferences.d/filedock
  fi
}

ui_address() {
  echo "${FD_UI_ADDRESS:-$(hostname -I 2>/dev/null | awk '{print $1}')}"
}

print_setup_code() {
  local code=""
  if command -v filedockctl >/dev/null 2>&1; then
    code="$(filedockctl setup-code --json 2>/dev/null | sed -n 's/.*"code":"\([^"]*\)".*/\1/p' || true)"
    if [[ -z "$code" ]]; then
      code="$(filedockctl setup-code 2>/dev/null | awk '/setup code/ {print $NF}' || true)"
    fi
  fi
  if [[ -z "$code" && -n "${FD_TEST_SETUP_CODE:-}" ]]; then
    code="$FD_TEST_SETUP_CODE"
  fi
  local addr
  addr="$(ui_address)"
  [[ -n "$addr" ]] || addr="127.0.0.1"
  echo "FileDock ${RELEASE} installed."
  echo "Open https://${addr}/ to finish setup."
  if [[ -n "$code" ]]; then
    echo "One-time setup code: $code"
  fi
}

if [[ "$REPAIR" -eq 1 ]]; then
  ensure_users
  ensure_paths
  install_journald_cap
  install_tls
  install_packages || exit 3
  echo "FileDock ${RELEASE} repaired. Configuration and user data were left unchanged."
  exit 0
fi

ensure_users
ensure_paths
install_journald_cap
install_tls
install_packages || exit 3
mkdir -p /etc/filedock
date -u +"%Y-%m-%dT%H:%M:%SZ" >"$MARKER"
print_setup_code
exit 0
