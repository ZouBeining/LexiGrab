"""AnkiConnect 测试助手（仅本地回环）。"""

from __future__ import annotations

import json
import urllib.request
from typing import Any

ENDPOINT = "http://127.0.0.1:8765"
DECK = "LexiGrab E2E"
MODEL = "LexiGrab Basic"
FIELDS = ["Word", "Definition", "Sentence", "Source"]


def call(action: str, params: dict[str, Any] | None = None) -> Any:
    body = json.dumps({"action": action, "version": 6, "params": params or {}}).encode("utf-8")
    request = urllib.request.Request(ENDPOINT, data=body, headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(request, timeout=30) as response:
        payload = json.loads(response.read().decode("utf-8"))
    if payload.get("error"):
        raise RuntimeError(f"AnkiConnect {action}: {payload['error']}")
    return payload.get("result")


def ensure_fixtures() -> None:
    if DECK not in call("deckNames"):
        call("createDeck", {"deck": DECK})
    if MODEL not in call("modelNames"):
        call(
            "createModel",
            {
                "modelName": MODEL,
                "inOrderFields": FIELDS,
                "css": ".card { font-family: arial; font-size: 20px; text-align: left; }",
                "isCloze": False,
                "cardTemplates": [
                    {
                        "Name": "Card 1",
                        "Front": "{{Word}}<br>{{Sentence}}",
                        "Back": "{{FrontSide}}<hr id=answer>{{Definition}}<br><i>{{Source}}</i>",
                    }
                ],
            },
        )


def notes(query: str) -> list[dict[str, Any]]:
    ids = call("findNotes", {"query": query})
    if not ids:
        return []
    return call("notesInfo", {"notes": ids})


def delete_notes(query: str) -> int:
    ids = call("findNotes", {"query": query})
    if ids:
        call("deleteNotes", {"notes": ids})
    return len(ids)


def latest_note() -> dict[str, Any] | None:
    found = notes(f'deck:"{DECK}"')
    if not found:
        return None
    return max(found, key=lambda note: note["noteId"])
