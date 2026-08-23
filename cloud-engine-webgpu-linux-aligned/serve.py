#!/usr/bin/env python3
"""Small no-dependency development server for the WebGPU cloud engine."""

from __future__ import annotations

import argparse
import functools
import mimetypes
import os
import threading
import webbrowser
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

mimetypes.add_type("text/plain; charset=utf-8", ".wgsl")


class CloudEngineHandler(SimpleHTTPRequestHandler):
    """Serve local assets without stale shader caching during development."""

    def end_headers(self) -> None:
        self.send_header("Cache-Control", "no-store")
        self.send_header("X-Content-Type-Options", "nosniff")
        super().end_headers()


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Serve Cloud Engine over localhost.")
    parser.add_argument("--host", default="127.0.0.1", help="Interface to bind (default: 127.0.0.1)")
    parser.add_argument("--port", type=int, default=8080, help="Port to bind (default: 8080)")
    parser.add_argument("--open", action="store_true", help="Open the page in the default browser")
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    root = Path(__file__).resolve().parent
    handler = functools.partial(CloudEngineHandler, directory=os.fspath(root))
    server = ThreadingHTTPServer((args.host, args.port), handler)
    display_host = "localhost" if args.host in {"127.0.0.1", "0.0.0.0", "::"} else args.host
    url = f"http://{display_host}:{args.port}/"
    print(f"Cloud Engine serving {root}")
    print(url)
    print("Press Ctrl+C to stop.")

    if args.open:
        threading.Timer(0.35, lambda: webbrowser.open(url)).start()

    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("\nStopping Cloud Engine.")
    finally:
        server.server_close()


if __name__ == "__main__":
    main()
