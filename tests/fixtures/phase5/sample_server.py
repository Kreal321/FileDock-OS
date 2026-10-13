#!/usr/bin/env python3
"""Disposable Phase 5 data plane: independent HTTP reads/writes and an injected crash switch."""
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path
import os
import sys
import threading
import time

root = Path(sys.argv[1])
root.mkdir(parents=True, exist_ok=True)

def crash_switch():
    while True:
        if (root / 'crash').exists(): os._exit(1)
        time.sleep(.1)

class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        data = (root/'value').read_bytes() if (root/'value').exists() else b'phase5-ready'
        self.send_response(200); self.end_headers(); self.wfile.write(data)
    def do_PUT(self):
        size = int(self.headers.get('Content-Length','0'))
        if size > 1024 * 1024:
            self.send_error(413); return
        data=self.rfile.read(size)
        temporary=root/'value.tmp';temporary.write_bytes(data);os.replace(temporary,root/'value')
        self.send_response(204); self.end_headers()

threading.Thread(target=crash_switch, daemon=True).start()
HTTPServer(('0.0.0.0',int(sys.argv[2])),Handler).serve_forever()
