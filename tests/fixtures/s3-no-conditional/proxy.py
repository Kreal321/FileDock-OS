#!/usr/bin/env python3
"""Test-only SigV4 proxy: ignore conditions and inject deterministic byte faults.
Never expose this server outside a disposable test network. /__fault accepts
{upload: flip|truncate|none, download: flip|truncate|none} and is unauthenticated.
Upstream credentials come from environment variables, never URL arguments.
"""
import datetime,hashlib,hmac,http.client,http.server,json,os,ssl,threading,time,urllib.parse,xml.etree.ElementTree as ET
UPSTREAM=urllib.parse.urlsplit(os.environ.get('FD_S3_UPSTREAM','https://127.0.0.1:9000'))
ACCESS=os.environ['FD_S3_ACCESS_KEY'];SECRET=os.environ['FD_S3_SECRET_KEY'];REGION=os.environ.get('FD_S3_REGION','us-east-1')
FAULT={'upload':'none','download':'none','ageMultipart':False,'holdPart':False};LOCK=threading.Lock();PART_SEEN=threading.Event()
def mutate(data,mode):
    if not data:return data
    if mode=='flip':return bytes([data[0]^1])+data[1:]
    if mode=='truncate':return data[:-1]
    return data

def sign(method,path,headers,body):
    now=datetime.datetime.now(datetime.timezone.utc);date=now.strftime('%Y%m%d');stamp=now.strftime('%Y%m%dT%H%M%SZ')
    headers={k.lower():v for k,v in headers.items() if k.lower() not in ('authorization','host','if-match','if-none-match','x-amz-date','x-amz-security-token','transfer-encoding','connection','content-length','expect')}
    headers.update(host=UPSTREAM.netloc,**{'x-amz-date':stamp,'x-amz-content-sha256':hashlib.sha256(body).hexdigest()})
    parsed=urllib.parse.urlsplit(path)
    query=sorted((urllib.parse.quote(k,safe='-_.~'),urllib.parse.quote(v,safe='-_.~')) for k,v in urllib.parse.parse_qsl(parsed.query,keep_blank_values=True) if not k.lower().startswith('x-amz-'))
    target=parsed.path+('?'+'&'.join(k+'='+v for k,v in query) if query else '')
    names=';'.join(sorted(headers));canonical=''.join(k+':'+ ' '.join(headers[k].split())+'\n' for k in sorted(headers))
    request='\n'.join([method,parsed.path or '/', '&'.join(k+'='+v for k,v in query),canonical,names,headers['x-amz-content-sha256']])
    scope=date+'/'+REGION+'/s3/aws4_request';message='AWS4-HMAC-SHA256\n'+stamp+'\n'+scope+'\n'+hashlib.sha256(request.encode()).hexdigest()
    key=('AWS4'+SECRET).encode()
    for value in (date,REGION,'s3','aws4_request'):key=hmac.new(key,value.encode(),hashlib.sha256).digest()
    signature=hmac.new(key,message.encode(),hashlib.sha256).hexdigest()
    headers['authorization']=f'AWS4-HMAC-SHA256 Credential={ACCESS}/{scope}, SignedHeaders={names}, Signature={signature}'
    return target,headers

class Proxy(http.server.BaseHTTPRequestHandler):
    protocol_version='HTTP/1.1'
    def log_message(self,*_):pass
    def body(self):
        if self.headers.get('Transfer-Encoding','').lower()=='chunked':
            chunks=[]
            while True:
                size=int(self.rfile.readline().split(b';')[0],16)
                if not size:self.rfile.readline();return b''.join(chunks)
                chunks.append(self.rfile.read(size));self.rfile.read(2)
        return self.rfile.read(int(self.headers.get('Content-Length','0')))
    def handle_request(self):
        body=self.body()
        if self.path=='/__fault':
            if self.command=='POST':
                try:
                    value=json.loads(body)
                    if any(k not in FAULT or (not isinstance(v,bool) if k in ('ageMultipart','holdPart') else v not in ('none','flip','truncate')) for k,v in value.items()):raise ValueError()
                    with LOCK:FAULT.update(value)
                    if value.get('holdPart'):PART_SEEN.clear()
                except (ValueError,TypeError):self.send_error(400);return
            with LOCK:data=json.dumps({**FAULT,'partSeen':PART_SEEN.is_set()}).encode()
            self.send_response(200);self.send_header('Content-Length',str(len(data)));self.end_headers();self.wfile.write(data);return
        with LOCK:fault=FAULT.copy()
        if self.command=='PUT':body=mutate(body,fault['upload'])
        path,headers=sign(self.command,self.path,dict(self.headers),body)
        connection=http.client.HTTPSConnection(UPSTREAM.hostname,UPSTREAM.port or 443,context=ssl.create_default_context(cafile=os.environ.get('FD_S3_CA')),timeout=30) if UPSTREAM.scheme=='https' else http.client.HTTPConnection(UPSTREAM.hostname,UPSTREAM.port or 80,timeout=30)
        try:
            connection.request(self.command,path,body=body,headers=headers);response=connection.getresponse();data=response.read()
            query=urllib.parse.parse_qs(urllib.parse.urlsplit(path).query,keep_blank_values=True)
            if fault['ageMultipart'] and self.command=='GET' and 'uploads' in query and response.status==200:
                xml=ET.fromstring(data)
                if xml.tag.startswith('{'):ET.register_namespace('',xml.tag[1:].split('}',1)[0])
                for node in xml.iter():
                    if node.tag.rsplit('}',1)[-1]=='Initiated':node.text=(datetime.datetime.now(datetime.timezone.utc)-datetime.timedelta(hours=25)).strftime('%Y-%m-%dT%H:%M:%S.000Z')
                data=ET.tostring(xml)
            if fault['holdPart'] and self.command=='PUT' and 'partNumber' in query and response.status==200:
                PART_SEEN.set()
                for _ in range(300):
                    with LOCK:holding=FAULT['holdPart']
                    if not holding:break
                    time.sleep(.1)
            if self.command=='GET' and response.status==200 and not urllib.parse.urlsplit(path).query:data=mutate(data,fault['download'])
            self.send_response(response.status)
            for key,value in response.getheaders():
                if key.lower() not in ('content-length','connection','transfer-encoding'):self.send_header(key,value)
            self.send_header('Content-Length',response.getheader('Content-Length','0') if self.command=='HEAD' else str(len(data)));self.end_headers()
            if self.command!='HEAD':self.wfile.write(data)
        except (OSError,http.client.HTTPException):self.send_error(502,'Fixture upstream failure')
        finally:connection.close()
    do_GET=do_PUT=do_POST=do_DELETE=do_HEAD=handle_request
if __name__=='__main__':
    server=http.server.ThreadingHTTPServer((os.environ.get('FD_S3_LISTEN','127.0.0.1'),int(os.environ.get('FD_S3_PORT','9443'))),Proxy)
    ctx=ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER);ctx.load_cert_chain(os.environ['FD_S3_CERT'],os.environ['FD_S3_KEY']);server.socket=ctx.wrap_socket(server.socket,server_side=True);server.serve_forever()
