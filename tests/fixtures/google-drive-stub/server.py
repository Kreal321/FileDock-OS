#!/usr/bin/env python3
"""Loopback-only Google Drive list stub. Fake tokens only; never forward traffic.

The caller supplies a short-lived test certificate and CA through the environment.
Logs contain method/path only. This is an opt-in fixture, not an appliance service.
"""
import http.server
import json
import os
import ssl
import re
import subprocess
import threading
import urllib.parse

context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
context.load_cert_chain(os.environ['FD_DRIVE_STUB_CERT'], os.environ['FD_DRIVE_STUB_KEY'])
count = 0
peers = []
lock = threading.Lock()


class Drive(http.server.BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_GET(self):
        global count
        if urllib.parse.urlsplit(self.path).path != '/drive/v3/files':
            self.send_error(404)
            return
        sockets = subprocess.check_output(
            ['ss', '-tnpH', 'sport', '=', ':' + str(self.client_address[1])], text=True
        )
        processes = re.findall(r'\("([^"]+)",pid=(\d+)', sockets)
        with lock:
            count += 1
            if len(peers) < 10000:
                peers.append(processes)
        payload = json.dumps({'kind': 'drive#fileList', 'files': []}).encode()
        self.send_response(200)
        self.send_header('Content-Type', 'application/json')
        self.send_header('Content-Length', str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)
        print(json.dumps({'method': 'GET', 'path': self.path.split('?')[0]}), flush=True)


class Proxy(http.server.BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_CONNECT(self):
        if self.path != 'www.googleapis.com:443':
            self.send_error(403, 'Fixture forwards no traffic')
            return
        self.send_response(200, 'Connection established')
        self.end_headers()
        with context.wrap_socket(self.connection, server_side=True) as stream:
            Drive(stream, self.client_address, self.server)
        self.close_connection = True

    def do_GET(self):
        if self.path != '/__count':
            self.send_error(404)
            return
        with lock:
            payload = json.dumps({'requests': count, 'peers': peers}).encode()
        self.send_response(200)
        self.send_header('Content-Type', 'application/json')
        self.send_header('Content-Length', str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)


if __name__ == '__main__':
    http.server.ThreadingHTTPServer(('127.0.0.1', 19405), Proxy).serve_forever()
