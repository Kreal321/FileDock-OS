# Agent instructions

## PVE development VM

Before connecting to the test VM, read `AGENTS.local.md` in this repository root for the
current network configuration, SSH/root credentials, FileDock administrator credentials,
upgrade commands, test procedures and pending cleanup. This file is local and Git-ignored.
Do not commit it or copy passwords into tracked reports, logs, command arguments or releases.
If it is absent, ask the user for the required connection details.

Record test evidence in `specs/001-core-os-foundation/`. Keep historical addresses/results
in older reports as historical evidence; use the local agent document for the current address.
Use uniquely named disposable resources and restore the baseline after testing. Full-disk,
disk-detach and destructive pool tests require a separate disposable VM and suitable disks.

## Development delivery

The user has authorized committing and pushing development fixes to `main`, which triggers
the offline bundle release workflow. Verify the workflow and installed `BUILD.txt` commit
before claiming a deployed fix passed. Upgrade using the VM's `filedock-bootstrap --update --yes`
when a complete release is published. See the local document for detailed checks.
