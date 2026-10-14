"""US4 canary checks across implemented public surfaces and encrypted copy files.
The diagnostic-bundle surface is added by US7; its check is collected separately.
"""
import json
import uuid
from state_helpers import setup,snapshot

def test_write_only_canaries_are_absent_from_history_logs_and_copies(installed_vm):
    appliance=setup(installed_vm);api=appliance.api
    values=['FD-CANARY-'+uuid.uuid4().hex for _ in range(5)]
    try:
        for kind,value in zip(['connection','backup','share','extension','notification'],values):
            api.put(f'/secrets/{kind}/canary/key',json={'value':value,'consumers':[]}).raise_for_status()
        snapshot(appliance)
        for path in ['/secrets','/audit','/system/status','/state-protection','/locations']:
            text=api.get(path).raise_for_status().text
            assert not any(value in text for value in values)
        logs=installed_vm.sudo('journalctl -u filedock-core -u filedock-management -u filedock-s3 -u filedock-rclone --no-pager').stdout
        assert not any(value in logs for value in values)
        history=installed_vm.sudo("python3 - <<'PY2'\nimport sqlite3,json\nc=sqlite3.connect('/var/lib/filedock-core/core.db');print(json.dumps(list(c.execute('SELECT payload FROM event'))))\nPY2").stdout
        assert not any(value in history for value in values)
        # Canaries are test-only values; inspect every local encrypted copy and spool byte.
        encoded=json.dumps(values)
        result=installed_vm.sudo("python3 - <<'PY2'\nfrom pathlib import Path\nimport json\nvalues=json.loads("+repr(encoded)+")\nfor root in ['/var/backups/filedock/appliance-state','/var/lib/filedock-core/appstate','/mnt/filedock/pools']:\n for file in Path(root).rglob('*'):\n  if file.is_file() and file.suffix in ['.fdstate','.fdpkg','.json']:\n   raw=file.read_bytes();assert not any(value.encode() in raw for value in values),'canary leak'\nprint('No canaries in local copies or spool')\nPY2")
        assert result.returncode==0
    finally:api.close()

def test_diagnostic_bundle_canary_surface_when_us7_is_available(installed_vm):
    import pytest
    appliance=setup(installed_vm)
    canary='FD-CANARY-diagnostic-'+uuid.uuid4().hex
    try:
        appliance.api.put('/secrets/extension/diagnostic/key',json={'value':canary,'consumers':[]}).raise_for_status()
        result=appliance.api.post('/diagnostics/bundle',json={})
        if result.status_code==404:pytest.skip('Diagnostic bundles are supplied by US7/Phase 9; this surface cannot be certified yet.')
        result.raise_for_status();assert result.content
        # Search the archive and its decompressed content; zstd is part of the supported VM.
        import base64
        data=base64.b64encode(result.content).decode()
        command="python3 - <<'PY2'\nimport base64,subprocess\nraw=base64.b64decode("+repr(data)+");decoded=subprocess.run(['zstd','--decompress','--stdout'],input=raw,capture_output=True,check=True).stdout;assert "+repr(canary.encode())+" not in decoded\nPY2"
        assert installed_vm.sudo(command).returncode==0
    finally:appliance.api.close()
