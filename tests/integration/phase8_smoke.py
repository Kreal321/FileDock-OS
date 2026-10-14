#!/usr/bin/env python3
"""Real Core + Management Phase 8 API guards, without launching OS runtimes.
Actual systemd/Docker isolation and rollback are exercised by tests/e2e/test_extensions.py.
"""
import hashlib
import http.cookies
import io
import json
from pathlib import Path
import subprocess
import tarfile
import tempfile
import urllib.error
import urllib.request
from phase6_7_smoke import Appliance

def upload(appliance,archive):
    boundary='fd-extension-smoke'
    body=f'--{boundary}\r\nContent-Disposition: form-data; name="file"; filename="sample.fdext"\r\nContent-Type: application/octet-stream\r\n\r\n'.encode()+archive+f'\r\n--{boundary}--\r\n'.encode()
    request=urllib.request.Request(f'http://127.0.0.1:{appliance.port}/api/v1/extensions/inspect',data=body,headers={'Content-Type':f'multipart/form-data; boundary={boundary}','Cookie':'; '.join(f'{k}={v}' for k,v in appliance.cookies.items()),'X-CSRF-Token':appliance.cookies.get('fd_http_csrf','')},method='POST')
    try:r=urllib.request.urlopen(request,timeout=30)
    except urllib.error.HTTPError as e:r=e
    return r.status,json.loads(r.read())

def package(bad=False):
    manifest=b'schemaVersion: 1\nid: sample-native\nname: Smoke sample\nversion: 1.0.0\nclass: extension\ncapabilities: [cache.generic]\nruntime: {type: native-process, executable: ./sample}\nhealth: {type: grpc}\npermissions: [core.events.read]\ncompatibility: {appliance: ">=0.1.0 <0.2.0"}\n'
    payload=b'#!/bin/sh\nexit 1\n'
    files={'manifest.yaml':manifest,'sample':payload}
    sums=''.join(hashlib.sha256(v).hexdigest()+'  '+k+'\n' for k,v in files.items()).encode()
    if bad:files['sample']=b'tampered'
    files['SHA256SUMS']=sums
    raw=io.BytesIO()
    with tarfile.open(fileobj=raw,mode='w') as tar:
        for name,content in files.items():
            entry=tarfile.TarInfo(name);entry.size=len(content);entry.mode=0o755;tar.addfile(entry,io.BytesIO(content))
    return subprocess.run(['zstd','--quiet','--stdout'],input=raw.getvalue(),capture_output=True,check=True).stdout

def main():
    with tempfile.TemporaryDirectory(prefix='fd-phase8-',dir='/tmp') as directory:
        root=Path(directory);appliance=Appliance(root/'appliance',root/'copy')
        try:
            appliance.setup()
            status,listed=appliance.call('GET','/api/v1/subsystems');assert status==200 and listed['subsystems']
            assert appliance.call('GET','/api/v1/extensions/folders')[0]==200
            status,invalid=upload(appliance,package(True));assert status==200 and not invalid['valid']
            status,inspected=upload(appliance,package());assert status==200 and inspected['valid'] and not inspected['signed']
            body={'stagedPackageId':inspected['stagedPackageId'],'approvedPermissions':[],'acknowledgedUnsigned':True,'dataBindings':{},'acknowledgedUnprotectedData':False}
            assert appliance.call('POST','/api/v1/extensions',body)[0]==422
            body['approvedPermissions']=inspected['requestedPermissions'];body['acknowledgedUnsigned']=False
            assert appliance.call('POST','/api/v1/extensions',body)[0]==422
            assert appliance.call('POST','/api/v1/extensions',dict(body,stagedPackageId='../../etc/shadow'))[0]==400
            assert appliance.call('DELETE','/api/v1/extensions/docker')[0]==422
            assert appliance.call('PUT','/api/v1/capabilities/cloud.remote/default',{'subsystemId':'fd-s3'})[0]==422
            assert appliance.call('PUT','/api/v1/capabilities/protocol.smb/default',{'subsystemId':'samba'})[0]==204
            defaults=appliance.call('GET','/api/v1/subsystems')[1]['defaultProviders'];assert defaults['protocol.smb']=='samba'
            assert not any(s['class']=='Extension' for s in appliance.call('GET','/api/v1/subsystems')[1]['subsystems'])
            print('PASS: real extension package inspect/integrity, exact permission and unsigned guards, staged path guard, built-in removal guard, folder candidates, generic defaults and cloud-route-only selection')
        finally:appliance.stop()
if __name__=='__main__':main()
