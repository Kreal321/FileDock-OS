#!/usr/bin/env bash
# Build filedock-core, filedock-management, and filedock-builtins .debs from $DIST.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
DIST="${1:-"$ROOT/dist"}"
VERSION="${FD_VERSION:-0.1.0}"
ARCH="${FD_ARCH:-amd64}"
STAGE="$DIST/debian-stage"
mkdir -p "$DIST" "$STAGE"

version_of() {
  awk -v name="$1" '$1=="deb" && $2==name { print $3; exit }' "$ROOT/packaging/versions.lock"
}

RCLONE_VER="$(version_of rclone || echo 1.75.2)"
KOPIA_VER="$(version_of kopia || echo 0.23.1)"

pkg_core="$STAGE/filedock-core"
pkg_mgmt="$STAGE/filedock-management"
pkg_bin="$STAGE/filedock-builtins"
rm -rf "$pkg_core" "$pkg_mgmt" "$pkg_bin"

install_bin() {
  local dest="$1" src="$2"
  if [[ -f "$src" ]]; then
    install -D -m 0755 "$src" "$dest"
  else
    echo "warning: missing $src (build the binary first)" >&2
    install -D -m 0755 /dev/null "$dest"
  fi
}

# --- filedock-core ---
mkdir -p "$pkg_core/DEBIAN" "$pkg_core/usr/lib/filedock" "$pkg_core/lib/systemd/system" \
  "$pkg_core/usr/share/filedock"
cat >"$pkg_core/DEBIAN/control" <<EOF
Package: filedock-core
Version: $VERSION
Section: admin
Priority: optional
Architecture: $ARCH
Maintainer: FileDock <ops@filedock.dev>
Depends: systemd
Description: FileDock Core Service
 Privileged reconciler, journal, secrets, and subsystem orchestrator.
EOF
install_bin "$pkg_core/usr/lib/filedock/filedock-core" "$ROOT/target/release/filedock-core"
install -m 0644 "$ROOT/packaging/systemd/filedock-core.service" "$pkg_core/lib/systemd/system/"
install -m 0644 "$ROOT/packaging/systemd/filedock-core.socket" "$pkg_core/lib/systemd/system/"
install -m 0644 "$ROOT/packaging/systemd/filedock-extensions.socket" "$pkg_core/lib/systemd/system/"

# --- filedock-management ---
mkdir -p "$pkg_mgmt/DEBIAN" "$pkg_mgmt/usr/lib/filedock" "$pkg_mgmt/usr/bin" \
  "$pkg_mgmt/lib/systemd/system"
cat >"$pkg_mgmt/DEBIAN/control" <<EOF
Package: filedock-management
Version: $VERSION
Section: admin
Priority: optional
Architecture: $ARCH
Maintainer: FileDock <ops@filedock.dev>
Depends: filedock-core (= $VERSION)
Description: FileDock Management Service and filedockctl
 Unprivileged web UI, REST API, and local admin CLI.
EOF
if [[ -x "$DIST/management/api/FileDock.Management.Api" ]]; then
  mkdir -p "$pkg_mgmt/usr/lib/filedock/management"
  cp -a "$DIST/management/api/." "$pkg_mgmt/usr/lib/filedock/management/"
  ln -sfn /usr/lib/filedock/management/FileDock.Management.Api \
    "$pkg_mgmt/usr/lib/filedock/filedock-management"
fi
if [[ -x "$DIST/management/cli/filedockctl" ]]; then
  install -D -m 0755 "$DIST/management/cli/filedockctl" "$pkg_mgmt/usr/bin/filedockctl"
fi
install -m 0644 "$ROOT/packaging/systemd/filedock-management.service" "$pkg_mgmt/lib/systemd/system/"

# --- filedock-builtins ---
mkdir -p "$pkg_bin/DEBIAN" "$pkg_bin/usr/lib/filedock" "$pkg_bin/usr/share/filedock/subsystems"
cat >"$pkg_bin/DEBIAN/control" <<EOF
Package: filedock-builtins
Version: $VERSION
Section: admin
Priority: optional
Architecture: $ARCH
Maintainer: FileDock <ops@filedock.dev>
Depends: filedock-core (= $VERSION), samba (= 2:4.22.11+dfsg-0+deb13u1), nfs-kernel-server (= 1:2.8.3-1), docker-ce, rclone, kopia
Description: FileDock built-in subsystems
 Pins Samba, NFS, Docker, rclone ${RCLONE_VER}, and Kopia ${KOPIA_VER}, and ships
 fd-s3 plus built-in manifests.
EOF
install_bin "$pkg_bin/usr/lib/filedock/fd-s3" "$ROOT/target/release/fd-s3"
if compgen -G "$ROOT/packaging/subsystems/*.yaml" >/dev/null; then
  install -m 0644 "$ROOT/packaging/subsystems/"*.yaml "$pkg_bin/usr/share/filedock/subsystems/"
fi

if command -v dpkg-deb >/dev/null; then
  dpkg-deb --build "$pkg_core" "$DIST/filedock-core_${VERSION}_${ARCH}.deb"
  dpkg-deb --build "$pkg_mgmt" "$DIST/filedock-management_${VERSION}_${ARCH}.deb"
  dpkg-deb --build "$pkg_bin" "$DIST/filedock-builtins_${VERSION}_${ARCH}.deb"
  echo "built packages in $DIST"
else
  echo "dpkg-deb is not installed; staged trees are in $STAGE"
fi
