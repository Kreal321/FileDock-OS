#!/usr/bin/env python3
"""Isolated Phase 4 native listener/console smoke check; never uses installed state.

Build management first. Override FD_PHASE4_SERVER with the Linux published API
executable to run without a machine-wide .NET SDK. Requires IPv6 loopback.
"""
import http.client, http.cookies, json, os, pathlib, socket, sqlite3, subprocess, tempfile, time, urllib.error, urllib.request
root = pathlib.Path(tempfile.mkdtemp(prefix='fd-p4-', dir=tempfile.gettempdir()))
sockpath = str(root/'admin.sock')
with socket.socket() as s:
    s.bind(('127.0.0.1', 0)); port=s.getsockname()[1]
env=dict(os.environ, Core__Optional='true', Database__Path=str(root/'app.db'), Kestrel__HttpPort=str(port), AdminSocket__Path=sockpath)
log=(root/'server.log').open('w')
server = [os.environ['FD_PHASE4_SERVER']] if os.environ.get('FD_PHASE4_SERVER') else ['dotnet', 'management/src/FileDock.Management.Api/bin/Debug/net10.0/FileDock.Management.Api.dll']
proc=subprocess.Popen(server, env=env, stdout=log, stderr=log)
cookies={}
def call(method,path,body=None,auth=True):
    headers={'Content-Type':'application/json'}
    if auth and cookies:
        headers['Cookie']='; '.join(f'{k}={v}' for k,v in cookies.items())
        headers['X-CSRF-Token']=cookies.get('fd_http_csrf','')
    request=urllib.request.Request(f'http://127.0.0.1:{port}'+path, data=None if body is None else json.dumps(body).encode(), headers=headers, method=method)
    try: response=urllib.request.urlopen(request, timeout=3)
    except urllib.error.HTTPError as error: response=error
    for value in response.headers.get_all('Set-Cookie') or []:
        jar=http.cookies.SimpleCookie(value)
        cookies.update({k:v.value for k,v in jar.items()})
    data=response.read()
    return response.status,json.loads(data) if data else None
class UnixConnection(http.client.HTTPConnection):
    def connect(self):
        self.sock=socket.socket(socket.AF_UNIX,socket.SOCK_STREAM);self.sock.connect(sockpath)
def console(path,body=None):
    conn=UnixConnection('localhost');conn.request('POST',path,None if body is None else json.dumps(body),{'Content-Type':'application/json'})
    response=conn.getresponse();data=response.read();status=response.status;conn.close()
    assert status==200,(path,status,data)
    return json.loads(data)
def ready():
    for _ in range(100):
        try:
            if call('GET','/health',auth=False)[0]==200 and pathlib.Path(sockpath).exists():
                if not cookies or call('GET','/api/v1/system/core-health')[1]['canMutate']: return
        except (OSError,urllib.error.URLError): pass
        if proc.poll() is not None: raise RuntimeError('Server exited')
        time.sleep(.1)
    raise RuntimeError('Server did not start')
def listening_v6():
    with socket.socket(socket.AF_INET6,socket.SOCK_STREAM) as s:
        s.settimeout(.2)
        return s.connect_ex(('::1',port))==0
try:
    ready()
    assert call('POST','/admin/setup-code',{})[0]==403
    code=console('/admin/setup-code')['code']
    status,key=call('POST','/api/v1/setup/complete',{'code':code,'applianceName':'phase4-test','adminUsername':'tester','adminPassword':'test-only','adminPasswordConfirmation':'test-only'})
    assert status==200
    assert call('POST','/api/v1/setup/recovery-key/confirm',{'keyId':key['keyId']})[0]==200
    ready()
    assert listening_v6()
    assert call('PUT','/api/v1/system/network',{'listenAddresses':['127.0.0.1']})[0]==200
    for _ in range(100):
        if not listening_v6(): break
        time.sleep(.1)
    assert not listening_v6(),'IPv6 listener was not removed'
    assert call('GET','/api/v1/system/status')[0]==200
    proc.terminate(); proc.wait(timeout=5)
    proc=subprocess.Popen(server, env=env, stdout=log, stderr=log)
    ready()
    assert not listening_v6(), 'Listener restriction did not survive restart'
    assert call('GET','/api/v1/system/status')[0]==200
    console('/admin/network/reset')
    for _ in range(100):
        if listening_v6(): break
        time.sleep(.1)
    assert listening_v6(),'Console did not restore all-interface listener'
    # A lost interface must not prevent root from restoring listener access.
    proc.terminate(); proc.wait(timeout=5)
    with sqlite3.connect(root/'app.db') as db:
        db.execute('UPDATE Settings SET ListenAddresses = ? WHERE Id = 1', (json.dumps(['192.0.2.1']),))
    proc=subprocess.Popen(server, env=env, stdout=log, stderr=log)
    for _ in range(100):
        try:
            console('/admin/network/reset')
            break
        except (OSError, http.client.HTTPException): time.sleep(.1)
    ready()
    assert listening_v6(), 'Console failed to recover a missing interface after restart'
    status,token=call('POST','/api/v1/tokens',{'name':'native-test','scope':'ReadOnly'})
    assert status==201
    console('/admin/tokens/revoke-all')
    headers={'Authorization':'Bearer '+token['token']}
    try: urllib.request.urlopen(urllib.request.Request(f'http://127.0.0.1:{port}/api/v1/system/status',headers=headers)); raise AssertionError('Revoked token accepted')
    except urllib.error.HTTPError as ex: assert ex.code==401
    console('/admin/reset-password',{'username':'tester','password':'changed-test-only'})
    assert call('GET','/api/v1/system/status')[0]==401
    cookies.clear()
    assert call('POST','/api/v1/auth/sign-in',{'username':'tester','password':'changed-test-only'})[0]==200
    status, audit = call('GET','/api/v1/audit')
    assert status==200
    assert {'network.reset', 'token.revoke-all', 'admin.password-reset'}.issubset({entry['action'] for entry in audit['items'] if entry['actorKind']=='Console'})
    print('PASS: real Kestrel restriction/reload, Unix console recovery, TCP console denial, token revocation, password reset/session invalidation, audit API')
finally:
    proc.terminate()
    try: proc.wait(timeout=5)
    except subprocess.TimeoutExpired: proc.kill(); proc.wait()
    log.close()
    print('Isolated test files:',root)
