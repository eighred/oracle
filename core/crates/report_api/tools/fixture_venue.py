"""Serve newly drawn, Eighred-owned BTCUSDT candles on a random loopback port."""
from contextlib import contextmanager
from http.server import BaseHTTPRequestHandler, HTTPServer
from threading import Thread
from urllib.parse import parse_qs, urlsplit
import json
import secrets
import time


@contextmanager
def fixture_venue():
    observations = {}

    class Venue(BaseHTTPRequestHandler):
        def log_message(self, *args):
            pass

        def do_GET(self):
            path = urlsplit(self.path)
            query = parse_qs(path.query)
            close = time.time_ns() // 60_000_000_000 * 60_000
            opened = close - 60_000
            expected = {"symbol": ["BTCUSDT"], "limit": ["1"], "interval": ["1m"],
                        "startTime": [str(opened)], "endTime": [str(close - 1)]}
            if path.path != "/api/v3/klines" or query != expected:
                self.send_error(400)
                return
            if opened not in observations:
                cents = 4_000_000 + secrets.randbelow(200_001)
                mark = f"{cents // 100}.{cents % 100:02d}"
                # One synthetic trade at this mark makes OHLC equal.
                observations[opened] = [[opened, mark, mark, mark, mark, "1.00",
                                          close - 1, mark, 1, "0", "0", "0"]]
            body = json.dumps(observations[opened], separators=(",", ":")).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

    with HTTPServer(("127.0.0.1", 0), Venue) as server:
        thread = Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            yield server.server_port, observations
        finally:
            server.shutdown()
            thread.join()
