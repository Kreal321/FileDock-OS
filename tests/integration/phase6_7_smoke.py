#!/usr/bin/env python3
"""Real Core + Management configuration snapshot/reinstall/file recovery smoke test.
Runs in temporary directories and never touches installed appliance state.
Build: cargo build -p filedock-core && dotnet build management/FileDock.Management.sln
Run: python3 tests/integration/phase6_7_smoke.py
"""
import base64
import http.client
import http.cookies
import json
import os
from pathlib import Path
import socket
import sqlite3
import subprocess
import tempfile
import time
import urllib.error
import urllib.request
REPO=Path(__file__).resolve().parents[2]
def wait(check,timeout=45):
    deadline=time.monotonic()+timeout
    while time.monotonic()<deadline:
        try:
            if check():return
        except (OSError,urllib.error.URLError):pass
        time.sleep(.1)
    raise AssertionError('Condition did not become true within the deadline')
class Appliance:
    def __init__(self,root,state_root):
        self.root=root;root.mkdir();self.cookies={};self.children=[];self.logs=(root/'logs').open('w')
        with socket.socket() as s:s.bind(('127.0.0.1',0));self.port=s.getsockname()[1]
        self.admin=str(root/'admin.sock');self.core=str(root/'core.sock')
        env=dict(os.environ,FD_CORE_SOCKET=self.core,FD_CORE_DB=str(root/'core.db'),FD_SECRETS=str(root/'secrets.enc'),FD_DEK=str(root/'local.key'),FD_RECOVERY_KEYS=str(root/'recovery.enc'),FD_MANIFESTS=str(REPO/'packaging/subsystems'),FD_SECRETS_RUN=str(root/'secrets'),FD_MGMT_UID=str(os.getuid()),FD_RUNTIME_ENABLED='0',FD_RECOVERY_ARGON_KIB='4096',FD_STATE_ROOT=str(state_root),FD_STATE_SPOOL=str(root/'spool'),FD_POOLS_ROOT=str(root/'pools'),FD_EXTENSION_ROOT=str(root/'extensions'),FD_EXTENSION_INBOX=str(root/'inbox'),FD_EXTENSION_RUN=str(root/'extension-run'),FD_EXTENSION_KEYS=str(root/'trusted-keys'))
        self.children.append(subprocess.Popen([str(REPO/'target/debug/filedock-core')],env=env,stdout=self.logs,stderr=self.logs,cwd=REPO))
        env=dict(os.environ,Core__Socket=self.core,Core__Optional='false',Database__Path=str(root/'app.db'),Kestrel__HttpPort=str(self.port),AdminSocket__Path=self.admin,Logging__LogLevel__Default='Warning',Extensions__Inbox=str(root/'inbox'))
        self.children.append(subprocess.Popen(['dotnet',str(REPO/'management/src/FileDock.Management.Api/bin/Debug/net10.0/FileDock.Management.Api.dll')],env=env,stdout=self.logs,stderr=self.logs,cwd=REPO))
        wait(lambda:self.call('GET','/health')[0]==200 and Path(self.admin).exists())
    def call(self,method,path,body=None,raw=False):
        headers={'Content-Type':'application/json','Cookie':'; '.join(f'{k}={v}' for k,v in self.cookies.items()),'X-CSRF-Token':self.cookies.get('fd_http_csrf','')}
        request=urllib.request.Request(f'http://127.0.0.1:{self.port}{path}',data=None if body is None else json.dumps(body).encode(),headers=headers,method=method)
        try:response=urllib.request.urlopen(request,timeout=30)
        except urllib.error.HTTPError as e:response=e
        for value in response.headers.get_all('Set-Cookie') or []:
            self.cookies.update({k:v.value for k,v in http.cookies.SimpleCookie(value).items()})
        data=response.read();return response.status,data if raw else json.loads(data) if data else None
    def file(self,archive,key,code):
        boundary='FileDockSmokeMultipart';parts=[]
        for name,value in [('setupCode',code),('recoveryKey',key)]:parts.append(f'--{boundary}\r\nContent-Disposition: form-data; name="{name}"\r\n\r\n{value}\r\n'.encode())
        parts.append(f'--{boundary}\r\nContent-Disposition: form-data; name="file"; filename="filedock.fdbackup"\r\nContent-Type: application/octet-stream\r\n\r\n'.encode()+archive+b'\r\n');parts.append(f'--{boundary}--\r\n'.encode())
        request=urllib.request.Request(f'http://127.0.0.1:{self.port}/api/v1/setup/restore/file',data=b''.join(parts),headers={'Content-Type':f'multipart/form-data; boundary={boundary}'},method='POST')
        try:response=urllib.request.urlopen(request,timeout=30)
        except urllib.error.HTTPError as e:response=e
        for value in response.headers.get_all('Set-Cookie') or []:self.cookies.update({k:v.value for k,v in http.cookies.SimpleCookie(value).items()})
        return response.status,json.loads(response.read())
    def console(self,path):
        class Unix(http.client.HTTPConnection):
            def connect(conn):conn.sock=socket.socket(socket.AF_UNIX);conn.sock.connect(self.admin)
        conn=Unix('localhost');conn.request('POST',path);response=conn.getresponse();data=response.read();conn.close();assert response.status==200,(response.status,data);return json.loads(data)
    def setup(self):
        code=self.console('/admin/setup-code')['code'];status,key=self.call('POST','/api/v1/setup/complete',dict(code=code,applianceName='recovery-smoke',adminUsername='root',adminPassword='test-only',adminPasswordConfirmation='test-only'));assert status==200,(status,key)
        assert self.call('POST','/api/v1/setup/recovery-key/confirm',{'keyId':key['keyId']})[0]==200
        wait(lambda:self.call('GET','/api/v1/system/core-health')[1].get('canMutate'))
        wait(lambda:self.call('POST','/api/v1/state-protection/snapshot')[0]==202)
        wait(lambda:bool(self.call('GET','/api/v1/state-protection')[1].get('copies')) and any(c.get('lastSnapshotAt') for c in self.call('GET','/api/v1/state-protection')[1]['copies']))
        return key
    def stop(self):
        for child in reversed(self.children):
            child.terminate()
            try:child.wait(timeout=5)
            except subprocess.TimeoutExpired:child.kill();child.wait()
        self.logs.close()
def main():
    with tempfile.TemporaryDirectory(prefix='fd-p67-',dir='/tmp') as directory:
        root=Path(directory);a=b=c=None
        try:
            a=Appliance(root/'a',root/'copy');key=a.setup()
            status,protection=a.call('GET','/api/v1/state-protection');assert status==200;assert not protection.get('machineLossCovered',False)
            index=next((root/'copy'/'appliance').glob('*/index.json'));before=json.loads(index.read_text())['snapshots']
            assert a.call('POST','/api/v1/auth/sign-out',{})[0] in (200,204)
            assert a.call('POST','/api/v1/auth/sign-in',{'username':'root','password':'test-only'})[0]==200
            time.sleep(5);assert json.loads(index.read_text())['snapshots']==before,'Sign-in/session writes triggered a snapshot'
            canaries=['FD-CANARY-cloud-a13','FD-CANARY-backup-b24','FD-CANARY-share-c35','FD-CANARY-extension-d46','FD-CANARY-notify-e57']
            for kind,value in zip(['connection','backup','share','extension','notification'],canaries):
                assert a.call('PUT',f'/api/v1/secrets/{kind}/smoke/key',{'value':value,'consumers':[]})[0]==200
            for path in ['/api/v1/secrets','/api/v1/audit','/api/v1/system/status']:
                status,response=a.call('GET',path);assert status==200
                assert not any(value in json.dumps(response) for value in canaries)
            status,archive=a.call('GET','/api/v1/state-protection/backup-file',raw=True);assert status==200
            assert b'test-only' not in archive
            for file in (root/'copy').rglob('*'):
                if file.is_file():assert not any(value.encode() in file.read_bytes() for value in canaries)
            assert not any(value in (root/'a'/'logs').read_text() for value in canaries)
            a.stop();a=None
            b=Appliance(root/'b',root/'copy')
            status,found=b.call('GET','/api/v1/setup/restore/local');assert status==200 and found['appliances']
            token=found['appliances'][0]['localSourceToken']
            assert b.call('POST','/api/v1/setup/restore/local',{'localSourceToken':token,'recoveryKey':'00000-00000-00000-00000-00000'})[0]==409
            status,source=b.call('POST','/api/v1/setup/restore/local',{'localSourceToken':token,'recoveryKey':key['recoveryKey']});assert status==200,(status,source)
            status,snapshots=b.call('GET','/api/v1/setup/restore/snapshots');assert status==200
            newest=next(s for s in snapshots['snapshots'] if s.get('newestValid'))
            status,op=b.call('POST','/api/v1/setup/restore',{'snapshotId':newest['snapshotId']});assert status==202,(status,op)
            wait(lambda:b.call('POST','/api/v1/auth/sign-in',{'username':'root','password':'test-only'})[0]==200)
            status,report=b.call('GET','/api/v1/restore/report');assert status==200,(status,report)
            with sqlite3.connect(root/'b'/'app.db') as db:
                assert db.execute('SELECT Action FROM AuditEntries ORDER BY At LIMIT 1').fetchone()[0]=='appliance.restore'
                assert db.execute('SELECT COUNT(*) FROM Administrators').fetchone()[0]==1
            assert b.call('POST','/api/v1/restore/report/dismiss',{})[0]==200
            assert b.call('GET','/api/v1/restore/report')[0]==204
            b.stop();b=None
            c=Appliance(root/'c',root/'new-copy');code=c.console('/admin/setup-code')['code']
            assert c.file(archive,key['recoveryKey'],'bad')[0]==400
            status,source=c.file(archive,key['recoveryKey'],code);assert status==200,(status,source)
            snapshots=c.call('GET','/api/v1/setup/restore/snapshots')[1];newest=next(s for s in snapshots['snapshots'] if s.get('newestValid'))
            status,op=c.call('POST','/api/v1/setup/restore',{'snapshotId':newest['snapshotId']});assert status==202
            wait(lambda:c.call('POST','/api/v1/auth/sign-in',{'username':'root','password':'test-only'})[0]==200)
            print('PASS: durable snapshot, sign-in exclusion, backup export, wrong key/code rejection, recovery-key-only reinstall, fresh-file restore, administrator import and retained report')
        finally:
            for appliance in [a,b,c]:
                if appliance:appliance.stop()
if __name__=='__main__':main()
