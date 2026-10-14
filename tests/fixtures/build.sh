#!/usr/bin/env bash
# Build test-only packages on the supported Debian amd64 host. Docker image stays local.
set -euo pipefail
FD_FIXTURE_ROOT=$(cd "$(dirname "$0")/../.." && pwd)
FD_FIXTURE_OUTPUT=${1:-"$FD_FIXTURE_ROOT/dist/fixtures"}
if [[ -z ${FD_FIXTURE_BINARY:-} ]]; then
  cargo build --release -p fd-extension-sample --manifest-path "$FD_FIXTURE_ROOT/Cargo.toml"
  FD_FIXTURE_BINARY="$FD_FIXTURE_ROOT/target/release/fd-extension-sample"
fi
[[ -x "$FD_FIXTURE_BINARY" ]] || { echo "Missing executable sample fixture: $FD_FIXTURE_BINARY" >&2; exit 1; }
mkdir -p "$FD_FIXTURE_OUTPUT"
FD_FIXTURE_OUTPUT=$(cd "$FD_FIXTURE_OUTPUT" && pwd)
FD_FIXTURE_STAGE=$(mktemp -d)
trap 'rm -rf "$FD_FIXTURE_STAGE"' EXIT
for name in sample-native sample-systemd sample-docker sample-native-broken notify-test; do
  if [[ "$name" == sample-docker && ${FD_FIXTURE_SKIP_DOCKER:-0} == 1 ]]; then continue; fi
  mkdir -p "$FD_FIXTURE_STAGE/$name"
  cp "$FD_FIXTURE_ROOT/tests/fixtures/$name/manifest.yaml" "$FD_FIXTURE_STAGE/$name/manifest.yaml"
  cp "$FD_FIXTURE_BINARY" "$FD_FIXTURE_STAGE/$name/sample"
  chmod 755 "$FD_FIXTURE_STAGE/$name/sample"
  if [[ $name == sample-systemd ]]; then cp "$FD_FIXTURE_ROOT/tests/fixtures/$name/sample.service" "$FD_FIXTURE_STAGE/$name/"; fi
  if [[ $name == sample-docker ]]; then
    mkdir -p "$FD_FIXTURE_STAGE/image"
    cp "$FD_FIXTURE_BINARY" "$FD_FIXTURE_STAGE/image/sample"
    # Copy the actual binary's shared-library closure; no mutable external base image.
    ldd "$FD_FIXTURE_STAGE/image/sample" | awk '/=> \// {print $3} /^\s*\// {print $1}' | while read -r lib; do
      mkdir -p "$FD_FIXTURE_STAGE/image$(dirname "$lib")"; cp -L "$lib" "$FD_FIXTURE_STAGE/image$lib"
    done
    FD_FIXTURE_LOADER=$(ldd "$FD_FIXTURE_STAGE/image/sample" | awk '/ld-linux/ {print $1}')
    if [[ -n $FD_FIXTURE_LOADER ]]; then mkdir -p "$FD_FIXTURE_STAGE/image$(dirname "$FD_FIXTURE_LOADER")"; cp -L "$FD_FIXTURE_LOADER" "$FD_FIXTURE_STAGE/image$FD_FIXTURE_LOADER"; fi
    printf 'FROM scratch\nCOPY . /\nENTRYPOINT ["/sample"]\n' > "$FD_FIXTURE_STAGE/image/Dockerfile"
    docker build -q -t filedock-sample-extension "$FD_FIXTURE_STAGE/image" >/dev/null
    FD_FIXTURE_IMAGE=$(docker image inspect --format '{{.Id}}' filedock-sample-extension)
    sed -i "s/IMAGE_DIGEST/$FD_FIXTURE_IMAGE/" "$FD_FIXTURE_STAGE/$name/manifest.yaml"
    docker save filedock-sample-extension > "$FD_FIXTURE_OUTPUT/sample-docker-image.tar"
  fi
  (cd "$FD_FIXTURE_STAGE/$name"; find . -type f ! -name SHA256SUMS -printf '%P\n' | sort | xargs sha256sum > SHA256SUMS; tar --zstd -cf "$FD_FIXTURE_OUTPUT/$name.fdext" $(find . -type f -printf '%P\n' | sort))
done
printf 'Test packages written to %s\n' "$FD_FIXTURE_OUTPUT"
