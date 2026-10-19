# Phase 12 implementation progress — 2026-10-19

## Status

**In progress; not ready to declare the Phase 12 independent test complete.**
Changes are local and uncommitted. No Phase 12 release has been published or
installed on the PVE VM. The current VM address remains in the ignored
`AGENTS.local.md`; no credentials are included here.

Completed implementation tasks: T164, T167, T169–T172. T165–T166, T168,
T173–T177 remain open until their remaining work and validation are complete.

## Implemented in the working tree

- Pure default/override route previews, validation issues, affected resources,
  credential recipients, lost features, mount reconciliation requests and safety
  acknowledgements. Overrides retain precedence over later default changes.
- Core route RPC wiring, generation checks, a single desired-state ApplyChange,
  acknowledgement/audit IDs and generic-CRUD bypass guards. Route reads/previews,
  mutation and session creation coordinate through the routing lock.
- Connector enable/disable guards, routed-by information, no-fallback availability
  checks and connector-specific unavailability alerts.
- Claim-safety observations per connector, recomputation and dashboard warning.
- Per-session native configuration/credential leases and rclone aliases. Live
  configuration changes replace the previous daemon restart. Obsolete ordinary
  credentials are removed while active leases retain their own lifetime. Only
  referenced uses receive ordinary credential materialization; connection probes
  use disposable leases.
- Native S3 storage class (single/multipart), SSE-KMS and a shared connection
  bandwidth bucket. Provider checksum capability now requires a checksum-specific
  negative challenge and a verified positive upload. Read-only probes do not write.
- Signed multipart listing/abort with age/prefix bounds; Core startup/daily sweep
  checks Writer ownership and skips reader/lost roles. A bucket-root sweep requires
  the explicit Writer-check flag. See `connectors/fd-s3/SPIKE.md`.
- Management endpoints, effective connector listings, root-console CLI commands
  and a Settings → Connectors & routes UI with preview and separate acknowledgements.
- Sample extension and fault proxy fixtures, plus release packaging for
  `test-fixtures/phase12/`. The sample inherits its instance ID/version/socket from
  the extension runtime and consumes the generic cloud configuration envelope.
- A narrow systemd ResetFailedUnit correction for unloaded units, identified while
  investigating the previously recorded native connector cold-start failure.

## Validation performed

- `cargo test --workspace --offline`: passed at the workspace checkpoint. Three
  ignored cases were reported: the live cross-connector test and two child-process
  helper entry points. This is not a live provider acceptance result.
- Subsequent targeted fd-connectors, fd-secrets and native S3 tests passed after
  additional changes; Core and native connector checks compiled successfully.
- Native tests cover forwarding storage class for both write paths, checksum
  rejection classification, read-back integrity, conditional writes, throttle
  sharing and sweep age/scope filtering.
- Ownership tests include 100 paired claims against an in-memory conditional store,
  plus stale Writer and reader/lost-role sweep refusal. These do not establish
  SC-025 against two real appliances/MinIO.
- `pnpm -C web test`: **56 tests passed across 18 files**. New coverage includes
  both acknowledgements, read-only inspection and preserving a failed apply draft.
- The Management API built successfully at an earlier checkpoint after compiler
  and analyzer fixes. Subsequent API/DTO/CLI changes and the new C# contract tests
  still require a fresh build and test run.
- Python syntax compilation and shell syntax checks passed for the new fixtures,
  acceptance files and packaging changes. `git diff --check` passed.
- The sample connector manifest validates; its cloud credential permission is
  checked explicitly in the manifest regression test.

## Resumed validation

The approval-service capacity failure is resolved. Production UI build and lint pass;
56 frontend tests, 26 Management API tests and 14 Management unit tests pass. The real
isolated Core/Management Phase 12 smoke test passes. A concurrent session-deletion race
found during API testing now returns unauthenticated rather than raising a database error.
The Makefile invokes the executable xUnit runners because the solution-level MTP command
was discovering zero tests. Desktop1440 and phone360 route-preview captures are in
`assets/phase12/`; acknowledgement gating and Escape/focus return were inspected.

The VM's MinIO source build completed from the official pinned tag
`RELEASE.2025-10-15T17-29-55Z`; Azurite is available at digest
`sha256:830430c1da1a2d537e08f3e6764dd1f5ae00cf0346bcaf625b968ec3f0971fd5`.
Live acceptance remains pending.

## Resume sequence

1. Finish any remaining acceptance coverage and inspect the full diff.
2. Run a fresh Management API/CLI build and .NET contract tests, frontend production
   build/type check/lint, and `tests/integration/phase12_smoke.py` against current
   Debug binaries. Fix any resulting issues before publishing.
3. Verify the connectors page/dialog at desktop and phone widths, including keyboard
   focus, outage/error states and truthful basic/third-party badges.
4. Build and inspect the Linux sample connector package. Install it only on a
   disposable test instance; verify sandbox secret visibility and live registration.
5. Commit/push the reviewed implementation to the authorized `main` branch, verify
   the offline release workflow, upgrade with the VM bootstrap, and verify BUILD.txt
   against that exact commit.
6. Run the complete quickstart Cloud connectors and routing steps 1–13. The new
   `tests/e2e/test_connectors.py` has initial route, switch/snapshot, foreign-owner,
   proxy, third-party and no-fallback cases. Coverage still needs completing for:
   dedicated backup-engine credential isolation; 100 real competing claims;
   download corruption and aborted multipart cleanup; Azurite; and restoring
   routes/acknowledgements/third-party package state. Verify every snapshot's
   decryption and cross-connector reads, not just object presence/ETags.
7. Verify the cold-start unit fix through reboot with no manual service starts.
   Clean up only uniquely identified test resources and restore the VM baseline.

Mount previews request folder reconciliation. Research R13 explicitly places the actual
CloudMount provider contract in feature004; feature001 reports non-local folder providers
as NotReady. A live CloudMount restart is deferred to that feature and is not claimed here.

The spec-kit extension hook file is absent, so there are no after-implement hooks
to dispatch at this checkpoint. Phase 12 has not been reported complete.
