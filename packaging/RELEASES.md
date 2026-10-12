# Downloadable offline releases

The workflow `.github/workflows/offline-release.yml` builds on Debian 13 amd64 and publishes a GitHub Release with:

- `filedock-offline-TAG-debian13-amd64.tar.gz`
- the matching `.tar.gz.sha256` checksum

The archive contains the three FileDock Debian packages, all pinned runtime packages and their dependencies, `install.sh`, `versions.lock`, `RELEASE_VERSION`, package checksums, and source-commit information. It excludes build trees and SDKs. The target VM does not need Rust, .NET SDK or Node.js.

## Generate a testing release

1. Push the project changes, including the Phase 1–3 packaging fixes and workflow, to GitHub. The workflow must be present on the default branch for its manual Run workflow button to appear.
2. Open **Actions → Offline bundle release → Run workflow**.
3. Choose the branch to build. Leave **Mark this release as a testing prerelease** enabled, then run it.
4. When both jobs succeed, open the release link in the run summary. Manual runs use a unique tag such as `test-0.1.0-12-1`.

Alternatively, pushing a version tag such as `v0.1.0` triggers a release. The tag must match the workspace version in `Cargo.toml`. Version-tag releases are published without the prerelease flag; releases are not automatically promoted to “latest”. A failed build publishes no release. Existing releases are not overwritten.

The workflow uses the built-in `GITHUB_TOKEN`; no personal token secret is required. Repository or organization policies must allow the publishing job's `contents: write` permission. Builds require access to Debian, Docker, GitHub, Rust, npm and .NET download servers. If an exact package pin becomes unavailable, the build fails rather than silently substituting another version.

## Download directly on the PVE test VM

Use a fresh **Debian 13 amd64 VM**, with 4 GB RAM, 2 vCPUs and a 256 GB sparse system disk. Start with the normal Debian package indexes and basic download tools; the FileDock installation uses the downloaded bundle without fetching runtime dependencies.

Replace the tag below with the actual tag shown in the release:

```bash
sudo apt-get update
sudo apt-get install -y curl ca-certificates
TAG=test-0.1.0-12-1
ARCHIVE="filedock-offline-${TAG}-debian13-amd64.tar.gz"
BASE="https://github.com/Kreal321/FileDock-OS/releases/download/${TAG}"
curl --fail --location --retry 3 --output "$ARCHIVE" "$BASE/$ARCHIVE"
curl --fail --location --retry 3 --output "$ARCHIVE.sha256" "$BASE/$ARCHIVE.sha256"
sha256sum -c "$ARCHIVE.sha256"
mkdir -p filedock-bundle
tar -xzf "$ARCHIVE" -C filedock-bundle
cd filedock-bundle
sudo bash install.sh --offline-bundle "$PWD" --check-only
sudo bash install.sh --offline-bundle "$PWD"
```

For a private repository, the anonymous curl URLs will not work. Use GitHub CLI on the VM after `gh auth login`, or download both assets through the authenticated GitHub UI and copy them to the VM:

```bash
gh release download "$TAG" --repo Kreal321/FileDock-OS \
  --pattern "$ARCHIVE" --pattern "$ARCHIVE.sha256"
```

The installer prints the HTTPS UI address and one-time setup code. Finish setup, save and confirm the recovery key, sign out and sign back in, then confirm all eight health cards are Healthy. Reboot and repeat the health check. See `specs/001-core-os-foundation/phase3-testing.md` for the full independent test.

A successful release build proves the build completed; it does not certify fresh-VM installation or subsystem health. The workflow has no VM acceptance stage.
