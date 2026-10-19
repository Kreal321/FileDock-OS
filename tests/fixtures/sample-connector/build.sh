#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/../../.." && pwd)
output=${1:-"$root/dist/fixtures"}
binary=${FD_S3_FIXTURE_BINARY:-"$root/target/release/fd-s3"}
[[ -x "$binary" ]] || { echo 'Build fd-s3 for the target architecture first.' >&2; exit 1; }
mkdir -p "$output"
output=$(cd "$output" && pwd)
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
cp "$root/tests/fixtures/sample-connector/manifest.yaml" "$stage/manifest.yaml"
cp "$binary" "$stage/sample"
chmod 755 "$stage/sample"
python3 - "$stage" <<'PY'
import hashlib, pathlib,sys
p=pathlib.Path(sys.argv[1]);(p/'SHA256SUMS').write_text(''.join(hashlib.sha256(f.read_bytes()).hexdigest()+'  '+f.name+'\n' for f in sorted(p.iterdir())))
PY
tar --zstd -C "$stage" -cf "$output/sample-connector.fdext" manifest.yaml sample SHA256SUMS
