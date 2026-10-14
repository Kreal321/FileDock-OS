#!/usr/bin/env python3
"""Isolated real Core -> gRPC relay -> SignalR smoke check; no installed state is touched."""
import http.client
import http.cookies
import json
import os
from pathlib import Path
import queue
import socket
import sqlite3
import subprocess
import tempfile
import threading
import time
import urllib.error
import urllib.request

REPO = Path(__file__).resolve().parents[2]
NODE_CLIENT = r'''
const {HubConnectionBuilder,LogLevel}=require(process.env.FD_SIGNALR_MODULE);
const c=new HubConnectionBuilder().withUrl(process.env.FD_SMOKE_URL+'/hubs/events',{headers:{Cookie:process.env.FD_SMOKE_COOKIE}}).withAutomaticReconnect([0,1000,1000,1000]).configureLogging(LogLevel.None).build();
for(const name of ['event','core-health','session-expired']) c.on(name,value=>console.log(JSON.stringify({name,value})));
c.onreconnected(()=>console.log(JSON.stringify({name:'reconnected'})));
c.onclose(()=>console.log(JSON.stringify({name:'closed'})));
c.start().then(()=>console.log(JSON.stringify({name:'connected'}))).catch(e=>{console.error(e.message);process.exit(1)});
process.on('SIGTERM',()=>c.stop().finally(()=>process.exit(0)));
'''

def wait_until(check, timeout=10):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            if check(): return
        except (OSError, urllib.error.URLError): pass
        time.sleep(.05)
    raise AssertionError('Condition did not become true')


def main():
    with tempfile.TemporaryDirectory(prefix='fd-p5-') as directory:
        root = Path(directory)
        # macOS Unix socket paths must stay below the platform limit.
        socket_path = str(root/'core.sock')
        admin_path = str(root/'admin.sock')
        with socket.socket() as sock:
            sock.bind(('127.0.0.1',0)); port=sock.getsockname()[1]
        core_env=dict(os.environ,FD_CORE_SOCKET=socket_path,FD_CORE_DB=str(root/'core.db'),FD_STATE_ROOT=str(root/'state-copy'),FD_STATE_SPOOL=str(root/'state-spool'),FD_POOLS_ROOT=str(root/'pools'),FD_SECRETS=str(root/'secrets.enc'),FD_DEK=str(root/'local.key'),FD_RECOVERY_KEYS=str(root/'recovery.enc'),FD_MANIFESTS=str(REPO/'packaging/subsystems'),FD_SECRETS_RUN=str(root/'secrets'),FD_MGMT_UID=str(os.getuid()),FD_RUNTIME_ENABLED='0',FD_RECOVERY_ARGON_KIB='4096')
        management_env=dict(os.environ,Logging__LogLevel__Default='Warning',Core__Socket=socket_path,Core__Optional='false',Database__Path=str(root/'app.db'),Kestrel__HttpPort=str(port),AdminSocket__Path=admin_path)
        core_command=[str(REPO/'target/debug/filedock-core')]
        management_command=['dotnet',str(REPO/'management/src/FileDock.Management.Api/bin/Debug/net10.0/FileDock.Management.Api.dll')]
        logs=(root/'logs').open('w')
        children=[]
        def launch(command,env):
            child=subprocess.Popen(command,env=env,stdout=logs,stderr=logs,cwd=REPO);children.append(child);return child
        cookies={}
        def call(method,path,body=None,authenticated=True):
            headers={'Content-Type':'application/json'}
            if authenticated:
                headers['Cookie']='; '.join(f'{k}={v}' for k,v in cookies.items())
                headers['X-CSRF-Token']=cookies.get('fd_http_csrf','')
            request=urllib.request.Request(f'http://127.0.0.1:{port}{path}',data=None if body is None else json.dumps(body).encode(),headers=headers,method=method)
            try: response=urllib.request.urlopen(request,timeout=60 if path=='/api/v1/setup/complete' else 5)
            except urllib.error.HTTPError as error: response=error
            for value in response.headers.get_all('Set-Cookie') or []:
                jar=http.cookies.SimpleCookie(value);cookies.update({k:v.value for k,v in jar.items()})
            data=response.read();return response.status,json.loads(data) if data else None
        messages=queue.Queue()
        def expect(name,predicate=lambda value:True,timeout=10):
            deadline=time.monotonic()+timeout
            while time.monotonic()<deadline:
                try: message=messages.get(timeout=max(.01,deadline-time.monotonic()))
                except queue.Empty: break
                if message['name']==name and predicate(message.get('value')): return message.get('value')
            raise AssertionError(f'SignalR message missing: {name}')
        try:
            core=launch(core_command,core_env);management=launch(management_command,management_env)
            wait_until(lambda:call('GET','/health',authenticated=False)[0]==200 and Path(admin_path).exists())
            assert call('GET','/api/v1/health',authenticated=False)[0]==200
            class UnixConnection(http.client.HTTPConnection):
                def connect(self):
                    self.sock=socket.socket(socket.AF_UNIX,socket.SOCK_STREAM);self.sock.connect(admin_path)
            conn=UnixConnection('localhost');conn.request('POST','/admin/setup-code');response=conn.getresponse();code=json.loads(response.read())['code'];conn.close()
            status,key=call('POST','/api/v1/setup/complete',{'code':code,'applianceName':'phase5-smoke','adminUsername':'tester','adminPassword':'test-only','adminPasswordConfirmation':'test-only'})
            assert status==200,(status,key)
            assert call('POST','/api/v1/setup/recovery-key/confirm',{'keyId':key['keyId']})[0]==200
            wait_until(lambda:call('GET','/api/v1/system/core-health')[1]['canMutate'])
            node_env=dict(os.environ,FD_SIGNALR_MODULE=str(REPO/'web/node_modules/@microsoft/signalr'),FD_SMOKE_URL=f'http://127.0.0.1:{port}',FD_SMOKE_COOKIE='; '.join(f'{k}={v}' for k,v in cookies.items()))
            node=subprocess.Popen(['node','-e',NODE_CLIENT],env=node_env,stdout=subprocess.PIPE,stderr=logs,text=True);children.append(node)
            def read_messages():
                for line in node.stdout:
                    messages.put(json.loads(line))
            threading.Thread(target=read_messages,daemon=True).start()
            expect('connected')
            before=time.monotonic();status,operation=call('POST','/api/v1/operations/reconcile',{})
            assert status==202,(status,operation)
            event=expect('event',lambda e:e['type']=='operation.completed')
            assert time.monotonic()-before<2,'event delivery exceeded two seconds'
            assert call('GET',f"/api/v1/operations/{operation['operationId']}")[1]['state']=='Succeeded'
            wait_until(lambda:sqlite3.connect(root/'app.db').execute('SELECT LastEventSeq FROM Settings').fetchone()[0]>=event['seq'])
            core.kill();core.wait(timeout=5)
            expect('core-health',lambda h:not h['canMutate'])
            assert call('POST','/api/v1/operations/reconcile',{})[0]==503
            assert call('GET','/health',authenticated=False)[0]==200
            core=launch(core_command,core_env)
            expect('core-health',lambda h:h['canMutate'],timeout=15)
            assert call('GET','/api/v1/system/status')[1]['applianceName']=='phase5-smoke'
            # Management restart must reconnect the browser and retain its durable cursor.
            management.terminate();management.wait(timeout=5)
            management=launch(management_command,management_env)
            expect('reconnected',timeout=15)
            expect('core-health',lambda h:h['canMutate'],timeout=15)
            assert call('GET','/api/v1/operations')[0]==200
            assert call('POST','/api/v1/auth/sign-out',{})[0]==204
            expect('session-expired',timeout=5)
            assert call('GET','/api/v1/system/status')[0]==401
            print('PASS: real Core gRPC resume, authenticated SignalR events under two seconds, durable cursor, Core loss/mutation guard/recovery, Management reconnect, session revocation, public health')
        except Exception:
            logs.flush();print((root/'logs').read_text()[-12000:]);raise
        finally:
            for child in reversed(children):
                if child.poll() is None:
                    child.terminate()
                    try:child.wait(timeout=5)
                    except subprocess.TimeoutExpired:child.kill();child.wait()
            logs.close()

if __name__=='__main__':main()
