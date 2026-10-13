# FileDock bootstrap installer and updater

`bootstrap.sh` is the small online entry point, published as `filedock-bootstrap.sh` with each GitHub release. The repository is currently hardcoded to [Kreal321/FileDock-OS](https://github.com/Kreal321/FileDock-OS/releases).

## On the VM

Download `filedock-bootstrap.sh` from the release assets. Run it **as root** (use `su -` if sudo is unavailable):

```bash
bash filedock-bootstrap.sh
```

The menu offers:

1. Install the newest release on a fresh host.
2. Update the existing installation.
3. Check the installed packages and release against the newest bundle.
4. Reinstall or repair the newest release.
0. Exit.

The bootstrap checks Debian 13 amd64, systemd, minimum RAM and system disk capacity. Missing curl, Python 3 and CA certificates are installed through APT. Version-check mode does not install prerequisites or update the appliance.

It selects the newest **published complete Debian 13 amd64 bundle**, including testing prereleases. `--stable` excludes prereleases. This uses the release list rather than GitHub's stable-only latest-release endpoint; see [GitHub's release API](https://docs.github.com/en/rest/releases/releases).

The archive and its SHA-256 file come from that release. The bootstrap verifies the digest, rejects unsafe archive paths or links, extracts into a private `/var/tmp` directory, checks the bundle identity and package checksums, and runs the bundled `install.sh`. Temporary downloads and extracted files are removed on exit. Installed configuration, administrator accounts, recovery state and storage are handled by the bundled installer's existing repair path.

After installation, the same script is available as `/usr/local/sbin/filedock-bootstrap`:

```bash
filedock-bootstrap --check
filedock-bootstrap --update
filedock-bootstrap --repair
```

For unattended use:

```bash
bash filedock-bootstrap.sh --install --yes
filedock-bootstrap --update --yes
```

`--yes` requires an explicit action for noninteractive execution. Checks compare all three FileDock package versions plus the installed release tag; testing releases that reuse `0.1.0` still replace the FileDock packages. An existing installation without recorded release identity is displayed as unknown and can be updated to establish that identity. Downgrades are refused.

Successful installations retain release provenance in `/etc/filedock/BUILD.txt`, `/etc/filedock/RELEASE_VERSION`, and (when using the bootstrap) `/etc/filedock/release.json`. The bootstrap record is written only after the bundled installer succeeds. Manual bundle installations also install the updater and record the included build identity.

## Publishing

The existing **Offline bundle release** workflow publishes:

- the offline `.tar.gz` and its `.sha256`;
- `filedock-bootstrap.sh` and its `.sha256`.

The archive includes `bootstrap.sh`, so the installed updater is refreshed with each new bundle. Its executable is replaced atomically, allowing an already-running updater to finish safely. Both scripts ship together; updates require the bundled installer changes that force FileDock package replacement when testing releases reuse a package version.

Bash and embedded Python syntax were checked locally. No installation or update was run against the PVE VM as part of this change.
