#!/usr/bin/env bash
# Build filedock-core, filedock-management, and filedock-builtins .debs from $DIST.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
DIST="${1:-"$ROOT/dist"}"
VERSION="${FD_VERSION:-0.1.0}"
ARCH="${FD_ARCH:-amd64}"
STAGE="$DIST/debian-stage"
if [[ "$(uname -s)" != Linux || "$(uname -m)" != x86_64 || "$ARCH" != amd64 ]]; then
  echo "Build Debian amd64 packages on a Linux amd64 host; native macOS/ARM Rust binaries are incompatible." >&2
  exit 1
fi
command -v dpkg-deb >/dev/null || { echo "Install dpkg-dev first." >&2; exit 1; }
for binary in "$ROOT/target/release/filedock-core" "$ROOT/target/release/fd-s3" \
  "$DIST/management/api/FileDock.Management.Api" "$DIST/management/cli/filedockctl"; do
  [[ -s "$binary" && -x "$binary" ]] || { echo "Missing executable: $binary" >&2; exit 1; }
  file "$binary" | grep -q 'ELF.*x86-64' || { echo "Not a Linux amd64 executable: $binary" >&2; exit 1; }
done
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
    echo "missing binary: $src" >&2
    exit 1
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
Depends: systemd, libc6, libgcc-s1
Description: FileDock Core Service
 Privileged reconciler, journal, secrets, and subsystem orchestrator.
EOF
install_bin "$pkg_core/usr/lib/filedock/filedock-core" "$ROOT/target/release/filedock-core"
install -m 0644 "$ROOT/packaging/systemd/filedock-core.service" "$pkg_core/lib/systemd/system/"
install -m 0644 "$ROOT/packaging/systemd/filedock-core.socket" "$pkg_core/lib/systemd/system/"
install -m 0644 "$ROOT/packaging/systemd/filedock-extensions.socket" "$pkg_core/lib/systemd/system/"

mkdir -p "$pkg_core/usr/lib/tmpfiles.d"
install -m 0644 "$ROOT/packaging/systemd/filedock.conf" "$pkg_core/usr/lib/tmpfiles.d/"

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
Depends: filedock-core (= $VERSION), libicu76, libssl3t64, libstdc++6, zlib1g
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
  mkdir -p "$pkg_mgmt/usr/lib/filedock/cli"
  cp -a "$DIST/management/cli/." "$pkg_mgmt/usr/lib/filedock/cli/"
  ln -s /usr/lib/filedock/cli/filedockctl "$pkg_mgmt/usr/bin/filedockctl"
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
Depends: filedock-core (= $VERSION), samba (= 2:4.22.11+dfsg-0+deb13u1), nfs-kernel-server (= 1:2.8.3-1), docker-ce (= $(awk '$1=="apt@docker" && $2=="docker-ce" {print $3}' "$ROOT/packaging/versions.lock")), docker-compose-plugin (= $(awk '$1=="apt@docker" && $2=="docker-compose-plugin" {print $3}' "$ROOT/packaging/versions.lock")), rclone (= $RCLONE_VER), kopia (= $KOPIA_VER)
Description: FileDock built-in subsystems
 Pins Samba, NFS, Docker, rclone ${RCLONE_VER}, and Kopia ${KOPIA_VER}, and ships
 fd-s3 plus built-in manifests.
EOF
mkdir -p "$pkg_bin/lib/systemd/system"
install -m 0644 "$ROOT/packaging/systemd/filedock-s3.service" "$pkg_bin/lib/systemd/system/"
install_bin "$pkg_bin/usr/lib/filedock/fd-s3" "$ROOT/target/release/fd-s3"
if compgen -G "$ROOT/packaging/subsystems/*.yaml" >/dev/null; then
  install -m 0644 "$ROOT/packaging/subsystems/"*.yaml "$pkg_bin/usr/share/filedock/subsystems/"
fi

if command -v dpkg-deb >/dev/null; then
  dpkg-deb --root-owner-group --build "$pkg_core" "$DIST/filedock-core_${VERSION}_${ARCH}.deb"
  dpkg-deb --root-owner-group --build "$pkg_mgmt" "$DIST/filedock-management_${VERSION}_${ARCH}.deb"
  dpkg-deb --root-owner-group --build "$pkg_bin" "$DIST/filedock-builtins_${VERSION}_${ARCH}.deb"
  echo "built packages in $DIST"
else
  echo "dpkg-deb is not installed; staged trees are in $STAGE"
fi

install -m 0755 "$ROOT/packaging/installer/install.sh" "$DIST/install.sh"
install -m 0755 "$ROOT/packaging/installer/bootstrap.sh" "$DIST/bootstrap.sh"
install -m 0644 "$ROOT/packaging/versions.lock" "$DIST/versions.lock"
printf '%s\n' "$VERSION" > "$DIST/RELEASE_VERSION"
(cd "$DIST" && sha256sum filedock-*.deb > SHA256SUMS)
