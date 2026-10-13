import os, subprocess, tempfile, time, socket
from pathlib import Path
import httpx
root=Path(__file__).resolve().parents[2]
with tempfile.TemporaryDirectory(prefix='fd-p3-') as scratch:
    d=Path(scratch); sock=socket.socket(); sock.bind(('127.0.0.1',0)); port=sock.getsockname()[1]; sock.close()
    env={**os.environ,'FD_CORE_SOCKET':str(d/'core.sock'),'FD_CORE_DB':str(d/'core.db'),'FD_SECRETS':str(d/'secrets.enc'),'FD_RECOVERY_KEYS':str(d/'recovery.enc'),'FD_DEK':str(d/'local.key'),'FD_MANIFESTS':str(root/'packaging/subsystems'),'FD_RUNTIME_ENABLED':'0','FD_MGMT_UID':str(os.getuid()),'FD_RECOVERY_ARGON_KIB':'8'}
    corelog=open(d/'core.log','w+'); applog=open(d/'api.log','w+')
    core=subprocess.Popen([str(root/'target/debug/filedock-core')],env=env,stdout=corelog,stderr=subprocess.STDOUT)
    appenv={**os.environ,'Core__Socket':str(d/'core.sock'),'Database__Path':str(d/'app.db'),'AdminSocket__Path':str(d/'admin.sock'),'Kestrel__HttpPort':str(port),'ASPNETCORE_CONTENTROOT':str(root/'management/src/FileDock.Management.Api')}
    app=subprocess.Popen(['dotnet',str(root/'management/src/FileDock.Management.Api/bin/Debug/net10.0/FileDock.Management.Api.dll')],env=appenv,stdout=applog,stderr=subprocess.STDOUT)
    try:
        client=httpx.Client(base_url=f'http://127.0.0.1:{port}')
        for _ in range(100):
            try:
                if client.get('/health').status_code==200: break
            except httpx.HTTPError: pass
            time.sleep(.1)
        else: raise RuntimeError('API startup failed')
        admin=httpx.Client(transport=httpx.HTTPTransport(uds=str(d/'admin.sock')),base_url='http://localhost')
        code=admin.post('/admin/setup-code').raise_for_status().json()['code']
        assert client.get('/').status_code==200
        assert client.get('/api/v1/system/status').status_code==423
        created=client.post('/api/v1/setup/complete',json={'code':code,'applianceName':'real-core-smoke','adminUsername':'admin','adminPassword':'Test-Password-123!','adminPasswordConfirmation':'Test-Password-123!'}).raise_for_status().json()
        headers={'X-CSRF-Token':client.cookies.get('fd_http_csrf')}
        client.post('/api/v1/setup/recovery-key/confirm',headers=headers,json={'keyId':created['keyId']}).raise_for_status()
        client.post('/api/v1/auth/sign-out',headers=headers).raise_for_status()
        assert client.get('/api/v1/system/status').status_code==401
        client.post('/api/v1/auth/sign-in',json={'username':'admin','password':'Test-Password-123!'}).raise_for_status()
        status=client.get('/api/v1/system/status').raise_for_status().json()
        assert status['applianceName']=='real-core-smoke'
        assert len(status['components'])==8
        print('PASS: real Rust Core + .NET API over Unix socket: setup, recovery confirmation, logout/login, dashboard status. Linux subsystem health requires VM.')
    except Exception:
        corelog.seek(0); applog.seek(0); print(corelog.read()); print(applog.read()); raise
    finally:
        for process in (app,core):
            process.terminate()
            try: process.wait(timeout=10)
            except subprocess.TimeoutExpired: process.kill(); process.wait()
