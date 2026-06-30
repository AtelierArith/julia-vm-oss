#!/usr/bin/env python3
"""Simple HTTP server with WASM MIME type support."""

import http.server
import socketserver

PORT = 8080

class WasmHandler(http.server.SimpleHTTPRequestHandler):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)

    extensions_map = {
        **http.server.SimpleHTTPRequestHandler.extensions_map,
        '.wasm': 'application/wasm',
        '.js': 'application/javascript',
        '.mjs': 'application/javascript',
    }

if __name__ == '__main__':
    socketserver.TCPServer.allow_reuse_address = True
    with socketserver.TCPServer(("", PORT), WasmHandler) as httpd:
        print(f"Serving at http://localhost:{PORT}")
        httpd.serve_forever()
