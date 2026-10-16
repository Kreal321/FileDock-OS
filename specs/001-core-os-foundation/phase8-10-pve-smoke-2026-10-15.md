# Phase 8 and Phase 10 PVE smoke — 2026-10-15

## Result

**Phase 10 is not accepted:** a reproduced Local Only bind-source defect puts writes on the OS
filesystem while reporting the folder Ready. Phase 8 package/API guards pass, but runtime
acceptance remains incomplete because the compiled extension fixtures and Docker image are absent.
No application source was changed during this test.

## Environment and isolation

- PVE VM `192.168.68.26`, Debian 13 amd64, kernel `6.12.111+deb13-amd64`.
- Installed packages report `0.1.0`; this version alone does not identify a release commit.
- Management HTTP port `8021`; administrator sign-in succeeded through API and browser.
- Core SHA256: `dc034ab879dc0eb408acb71d1c3c64c7b7342b8173f3796d1c734b15624fb984`.
- Management launcher SHA256: `f023549d85d4ea66d2c7813414170ca9afac0ae06a17752becfaf4b1cf5120b0`.
- Installed Core PID `71162`, Management PID `71172`; both retained these PIDs and `NRestarts=0`.
- Core, Management, Docker, fd-s3, Kopia, NFS, rclone and Samba were Healthy before and after testing.
- The installed appliance has no pools, managed folders, or installed third-party extensions.

Installed-service checks used package inspection and refused API mutations. Real mount execution
used separate installed-binary Core and Management processes with private databases, sockets,
secrets, state directories, an empty subsystem registry, and temporary HTTP port `18081`.
A synthetic pool resource existed only in the isolated database. Its filesystem was a new
64 MiB disposable loop-mounted ext4 image, not a physical disk or an installed-appliance pool.

## Phase 8

| Check | Result |
| --- | --- |
| Tampered payload rejected | Pass: `extension.integrity_failed` |
| Archive path traversal rejected | Pass: `extension.path_invalid` |
| Invalid signature rejected | Pass: `extension.signature_invalid` |
| Missing exact permission approval refused | Pass: HTTP 422 |
| Missing unsigned-package acknowledgement refused | Pass: HTTP 422 |
| Built-in Docker removal refused | Pass: `extension.not_removable` |
| Generic cloud default cannot bypass connector routes | Pass: `capability.requires_connector_route` |
| Browser extension list and built-in runtime state | Pass: six running built-ins; no removal controls |
| Native/systemd/Docker extension execution and sandbox | Not run: compiled sample packages/image unavailable |
| Twenty kills, extension backoff, Core independence | Not run: compiled sample package unavailable |
| Broken upgrade rollback | Not run: compiled sample and broken-upgrade packages unavailable |
| Package snapshot deduplication, pruning and recovery | Not run: no installed sample extension; no configured recovery pool |
| Folder bindings and extension deletion dependencies | Not run: no compiled runtime fixture |

The earlier negative-package fixtures under `/var/tmp/filedock-phase8-20261014` were used only for
inspection/refusal checks. They are not functioning runtime fixtures and were not installed.
The release extraction directory has been cleaned by the installer; neither the VM nor local
workspace contained the compiled Phase 8 fixtures. An unauthenticated GitHub latest-release API
request returned 404, and the user did not know the archive location. This does not certify that
the published release lacks fixtures; its archive was unavailable for inspection.

## Phase 10

| Check | Result |
| --- | --- |
| Browser folder editor | Pass: one page with ordered sections and collapsed Advanced |
| Generated read-only path preview | Pass: `Phase 10 UI Check` → `/storage/phase-10-ui-check`; cancelled without saving |
| Unavailable modes identify missing capabilities | Pass: Cloud Mount, Hybrid Backup, Tiered Storage disabled |
| Empty pool list explains dependency | Pass: UI explains storage phase dependency |
| Nonexistent pool validation | Pass: `reference.pool_missing` |
| Server ignores custom FileDock path in favor of name | Pass |
| Local Only exposure | Pass: Unprotected with explicit pool-loss warning |
| Pool root, internal paths, traversal | Pass: rejected |
| Duplicate name and nested local reference | Pass: rejected |
| Missing/stale generation and immutable reference edits | Pass: HTTP 428 / 409 / 422 respectively |
| Actual bind source and cross-path data verification | **Fail: see blocking defect below** |
| Stop isolated Management while folder is mounted | Pass: existing mount/I/O survives; installed Management untouched |
| Stop disposable pool mount | Pass: folder becomes Unavailable and folder mount stops |
| Reattach same disposable pool identity | Partial: Ready/mount returns and target canary survives, but on the wrong backing filesystem |
| Delete unused folder | Mount removal passes; pool-data preservation acceptance blocked by wrong-source defect |
| Hybrid importance/default/confirmation and typed Critical deletion | Not run: required mode provider absent |
| Cloud/Tiered execution and protection observations | Not run: later feature providers absent |
| Applications/export/extension dependency deletion guards | Not exercised with live dependencies |

## Blocking defect: quoted bind source resolves onto the OS disk

Reproduced three times using fresh isolated databases and different disposable pool identities.

1. Mount a real disposable ext4 pool under `/mnt/filedock/pools/<uuid>` and provide its pool marker.
2. Create Local Only with reference `/movies` and generated target
   `/storage/fd-phase10-20261015-movies`.
3. Wait for the folder status to become Ready.
4. Compare both mount sources and write `canary.txt` through the folder target.

Observed:

```text
pool:   /dev/loop0
folder: /dev/sda1[/"/mnt/filedock/pools/<uuid>/movies"]
```

The canary appears in the unintended OS-disk directory
`/"/mnt/filedock/pools/<uuid>/movies"/canary.txt`, and is absent from the real pool's
`movies/canary.txt`. Both filesystems are ext4, so checking only the filesystem type would miss this.

The generated mount unit contains:

```ini
[Mount]
What="/mnt/filedock/pools/<uuid>/movies"
Where="/storage/fd-phase10-20261015-movies"
Type=none
Options=bind
```

The runtime evidence indicates that the quote characters in `What` are treated literally here.
The corresponding source is `core/crates/fd-reconcile/src/controllers/folder.rs`, in `quote()`
and `unit()`. Source rendering and post-start verification need correction: verify that the target
actually maps to the intended source directory/device before declaring Ready.
Retest real pool/target data equivalence, deletion preservation, and pool-loss recovery afterward.

## Cleanup and limits

- Stopped all isolated processes; port `18081` has no remaining listener.
- Removed owned mount units, mounts, empty mountpoints, disposable images, and the exact stray
  quoted-path canaries/directories created by these tests.
- Removed isolated databases, secrets and setup state after collecting sanitized evidence.
- Signed out the API test session and removed its cookie file.
- The installed folder list and third-party extension list remain empty; installed services remain healthy.
- Authentication and refused test requests naturally add audit/journal records.
- No real disk was formatted; no installed-appliance pool or folder records were inserted.

These are targeted VM smoke checks, not complete Phase 8/10 acceptance. Fix the mount defect and
obtain the published compiled fixtures before treating these phases as delivered.
