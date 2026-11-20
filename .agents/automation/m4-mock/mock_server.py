"""M4 手工验收用的本地 mock：远程词典、JEV-like 选择器、OpenAI-compatible LLM。

用法：`python mock_server.py [port]`，默认 127.0.0.1:18080。
所有响应只用于本地验收，不含任何真实数据。
"""

import json
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, urlparse

SELECTED_HIGH = 0.81
SELECTED_LOW = 0.21


class Handler(BaseHTTPRequestHandler):
    def log_message(self, fmt, *args):  # noqa: A003 - 只写 stderr 的简短访问日志
        sys.stderr.write("[mock] %s\n" % (fmt % args))

    def _send(self, status, payload, content_type="application/json"):
        body = payload if isinstance(payload, bytes) else json.dumps(payload).encode()
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):  # noqa: N802 - http.server 约定
        parsed = urlparse(self.path)
        query = parse_qs(parsed.query)
        word = query.get("q", ["bank"])[0]
        if word == "no-such-word-xyz":
            self._send(200, {"entries": [], "items": []})
            return
        if parsed.path == "/dict":
            self._send(
                200,
                {
                    "entries": [
                        {
                            "headword": word,
                            "definition_html": (
                                "<p>financial institution</p>"
                                "<script>alert(1)</script>"
                                "<img src=\"relative.png\">"
                                "<img src=\"https://example.com/remote.png\">"
                            ),
                            "phonetic": "/bæŋk/",
                            "resources": ["/relative.png", "https://example.com/remote.png"],
                        },
                        {
                            "word": word,
                            "definition_html": "<ol><li>sloping land beside a river</li></ol>",
                        },
                    ]
                },
            )
            return
        if parsed.path == "/script":
            self._send(
                200,
                {
                    "items": [
                        {
                            "word": word,
                            "html": "<p>script parsed definition</p>",
                            "phonetic": "/skrɪpt/",
                        }
                    ]
                },
            )
            return
        if parsed.path == "/fail":
            self._send(500, {"error": "boom"})
            return
        self._send(404, {"error": "not found"})

    def do_POST(self):  # noqa: N802
        length = int(self.headers.get("Content-Length", "0"))
        raw = self.rfile.read(length)
        try:
            request = json.loads(raw or b"{}")
        except json.JSONDecodeError:
            request = {}
        path = urlparse(self.path).path
        if path == "/v1/select":
            sentence = str(request.get("sentence", ""))
            confidence = SELECTED_HIGH if "river" in sentence.lower() else SELECTED_LOW
            senses = request.get("senses") or []
            selected = senses[1]["id"] if len(senses) > 1 and confidence >= 0.72 else (senses[0]["id"] if senses and confidence > 0.55 else None)
            self._send(
                200,
                {
                    "selected_id": selected,
                    "confidence": confidence,
                    "distribution": {sense["id"]: 1.0 / max(1, len(senses)) for sense in senses},
                    "model": "jev-like-mock",
                    "reason": "river context" if confidence >= 0.72 else "weak context",
                },
            )
            return
        if path == "/v1/chat/completions":
            content = json.dumps(
                {
                    "definition": "the land alongside a river",
                    "part_of_speech": "noun",
                    "confidence": 0.7,
                    "explanation": "the sentence mentions the river",
                }
            )
            self._send(
                200,
                {
                    "id": "mock",
                    "choices": [{"message": {"role": "assistant", "content": content}}],
                },
            )
            return
        self._send(404, {"error": "not found"})


def main():
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 18080
    server = ThreadingHTTPServer(("127.0.0.1", port), Handler)
    print(f"READY {port}", flush=True)
    server.serve_forever()


if __name__ == "__main__":
    main()
