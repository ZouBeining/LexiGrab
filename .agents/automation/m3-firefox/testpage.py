"""本地测试页：确定性覆盖中英文句子、同词多句与站点矩阵。"""

from __future__ import annotations

import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

PAGES: dict[str, tuple[str, str]] = {
    "/en": (
        "LexiGrab English fixture",
        """<!doctype html><html lang="en"><head><meta charset="utf-8"><title>LexiGrab English fixture</title></head>
<body>
<article>
<h1>Hello world</h1>
<p id="s1">Hello there, this is a quiet English page for LexiGrab.</p>
<p id="s2">The world is wide, and every world has its own story.</p>
<p id="s3">She said hello to the world and kept walking.</p>
<p id="s4">A lexigrab demo sentence lives here, with <b>bold</b> and <a href="#">a link</a>.</p>
</article>
</body></html>""",
    ),
    "/zh": (
        "LexiGrab 中文测试页",
        """<!doctype html><html lang="zh-CN"><head><meta charset="utf-8"><title>LexiGrab 中文测试页</title></head>
<body>
<article>
<h1>你好，世界</h1>
<p id="z1">你好，hello 在这个中文页面里出现。世界很大，world 也在句子里。</p>
<p id="z2">他回头说了一声 hello，然后继续赶路。</p>
<p id="z3">这是第三个句子，用来验证中文标点切分。</p>
</article>
</body></html>""",
    ),
    "/news": (
        "World news fixture",
        """<!doctype html><html lang="en"><head><meta charset="utf-8"><title>World news fixture</title></head>
<body>
<main>
<h1>World leaders meet for climate talks</h1>
<p id="n1">World leaders gathered on Monday to discuss the climate crisis.</p>
<p id="n2">The world economy is expected to slow next year.</p>
<p id="n3">Hello from the newsroom: this is a generated test page.</p>
</main>
</body></html>""",
    ),
    "/multi": (
        "LexiGrab multi-sentence fixture",
        """<!doctype html><html lang="en"><head><meta charset="utf-8"><title>LexiGrab multi-sentence fixture</title></head>
<body>
<article>
<h1>Same word, different sentences</h1>
<p id="m1">Hello, can you hear me?</p>
<p id="m2">She said hello to the world.</p>
<p id="m3">Hello again, old friend.</p>
</article>
</body></html>""",
    ),
}


class _Handler(BaseHTTPRequestHandler):
    def do_GET(self) -> None:  # noqa: N802 - http.server 约定
        page = PAGES.get(self.path.split("?")[0])
        if page is None:
            self.send_response(404)
            self.end_headers()
            self.wfile.write(b"not found")
            return
        body = page[1].encode("utf-8")
        self.send_response(200)
        self.send_header("Content-Type", "text/html; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, format: str, *args: object) -> None:  # noqa: A003
        return


class TestPageServer:
    def __init__(self, port: int = 8766) -> None:
        self.port = port
        self._server: ThreadingHTTPServer | None = None
        self._thread: threading.Thread | None = None

    def start(self) -> str:
        self._server = ThreadingHTTPServer(("127.0.0.1", self.port), _Handler)
        self._thread = threading.Thread(target=self._server.serve_forever, daemon=True)
        self._thread.start()
        return f"http://127.0.0.1:{self.port}"

    def stop(self) -> None:
        if self._server is not None:
            self._server.shutdown()
            self._server.server_close()
            self._server = None
        if self._thread is not None:
            self._thread.join(timeout=5)
            self._thread = None

    def url(self, path: str) -> str:
        return f"http://127.0.0.1:{self.port}{path}"
