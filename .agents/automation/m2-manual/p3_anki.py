"""M2 manual matrix, part 3: real Anki card creation and error states."""

from __future__ import annotations

import json
import re
import subprocess
import sys
import time
import urllib.request
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import common as c
import p2_popup as p2

RESULT: dict = {"phase": "anki", "steps": [], "notes": {}}
ANKI_EXE = Path(r"D:\Anki\Anki.exe")
ENDPOINT = "http://127.0.0.1:8765"
DECK = "LexiGrab E2E"
MODEL = "LexiGrab Basic"


def step(name: str, fn):
    print(f"\n=== {name} ===")
    value = fn()
    entry = {"step": name, "ok": True}
    if value is not None:
        entry["detail"] = value
    RESULT["steps"].append(entry)
    print(f"--- {name}: OK")
    return value


def anki(action: str, **params):
    body = {"action": action, "version": 6, "params": params}
    request = urllib.request.Request(
        ENDPOINT,
        data=json.dumps(body).encode("utf-8"),
        headers={"Content-Type": "application/json"},
    )
    with urllib.request.urlopen(request, timeout=10) as response:
        payload = json.loads(response.read().decode("utf-8"))
    if payload.get("error"):
        raise RuntimeError(f"AnkiConnect {action}: {payload['error']}")
    return payload["result"]


def anki_up(timeout: float = 2.0) -> bool:
    try:
        urllib.request.urlopen(
            urllib.request.Request(
                ENDPOINT,
                data=json.dumps({"action": "version", "version": 6}).encode("utf-8"),
                headers={"Content-Type": "application/json"},
            ),
            timeout=timeout,
        )
        return True
    except Exception:
        return False


def ensure_anki(timeout: float = 90.0) -> bool:
    if anki_up():
        return True
    subprocess.Popen(
        [str(ANKI_EXE)],
        stdin=subprocess.DEVNULL,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    return bool(c.wait_for(anki_up, timeout=timeout, interval=0.5))


def deck_notes() -> list[int]:
    return anki("findNotes", query=f'deck:"{DECK}"')


def notes_info(ids: list[int]) -> list[dict]:
    return anki("notesInfo", notes=ids)


def field_value(note: dict, name: str) -> str:
    return note["fields"].get(name, {}).get("value", "")


def configure_anki_settings() -> None:
    c.set_config_value("deck", f'"{DECK}"', "anki")
    c.set_config_value("model", f'"{MODEL}"', "anki")


def popup_win(app, pid: int, texts: list[str], timeout: float = 10.0):
    """Wait until all `texts` appear in the popup (substring match)."""
    end = time.perf_counter() + timeout
    while time.perf_counter() < end:
        win = c.popup_window(pid)
        if win and win.visible:
            current = c.uia_texts(app.window(handle=win.hwnd))
            if all(any(needle in t for t in current) for needle in texts):
                return win, current
        time.sleep(0.15)
    win = c.popup_window(pid)
    current = c.uia_texts(app.window(handle=win.hwnd)) if win and win.visible else []
    raise AssertionError(f"popup texts missing {texts}; current={current[:40]}")


def lookup(app, pid: int, np, text: str, first_word: bool = False):
    p2.notepad_set(np, text)
    if first_word:
        p2.select_first_word(np)
    else:
        p2.select_all(np)
    win, _ = p2.trigger_and_wait(pid)
    win, texts = popup_win(app, pid, ["保存所选义项"])
    time.sleep(0.3)
    return win, texts


def click_save(app, win) -> None:
    spec = app.window(handle=win.hwnd)
    spec.child_window(title="保存所选义项", control_type="Button").click_input()


def wait_saved(app, pid: int, timeout: float = 15.0):
    win, texts = popup_win(app, pid, ["已保存到 Anki"], timeout=timeout)
    time.sleep(0.3)
    texts = c.uia_texts(app.window(handle=win.hwnd))
    return win, texts


def note_id_from_texts(texts: list[str]) -> int | None:
    for index, text in enumerate(texts):
        match = re.search(r"note\s+(\d+)", text)
        if match:
            return int(match.group(1))
        if text.strip() == "note" and index + 1 < len(texts):
            candidate = texts[index + 1].strip()
            if candidate.isdigit():
                return int(candidate)
    return None


def close(app, pid: int) -> None:
    p2.close_popup(app, pid)


def main() -> int:
    assert ensure_anki(), "Anki / AnkiConnect not reachable"
    RESULT["anki_version"] = anki("version")
    baseline = deck_notes()
    if baseline:
        anki("deleteNotes", notes=baseline)
        baseline = deck_notes()
    RESULT["baseline_notes"] = baseline
    configure_anki_settings()
    c.kill_app()
    # Start every run with an empty saved_cards history (M4 has no clear command yet).
    for cache in c.HOME.glob("cache.db*"):
        cache.unlink(missing_ok=True)
    info = c.launch_app()
    RESULT["launch"] = info
    pid = c.app_pid()
    app = c.connect(pid)
    _, np = p2.notepad(reset=True)
    p2.popup_hidden(pid)
    print("app pid:", pid, "deck:", DECK, "model:", MODEL)

    def success():
        win, texts = lookup(app, pid, np, "hello")
        click_save(app, win)
        win, saved_texts = wait_saved(app, pid)
        note_id = note_id_from_texts(saved_texts)
        assert note_id, saved_texts
        c.screenshot_window(win, "30-anki-saved.png")
        ids = deck_notes()
        assert note_id in ids, (note_id, ids)
        note = notes_info([note_id])[0]
        fields = {name: field_value(note, name) for name in ("Word", "Definition", "Sentence", "Source")}
        assert fields["Word"] == "hello", fields
        assert fields["Sentence"] == "hello", fields
        assert fields["Source"] == "mini", fields
        assert "greeting" in fields["Definition"], fields["Definition"][:120]
        assert "global-dict" in note["tags"], note["tags"]
        assert note["modelName"] == MODEL, note["modelName"]
        close(app, pid)
        return {"note_id": note_id, "fields": fields, "tags": note["tags"], "count": len(ids)}

    step("create a card through the popup (real Anki)", success)

    def same_context():
        win, texts = lookup(app, pid, np, "world")
        click_save(app, win)
        win, _ = wait_saved(app, pid)
        close(app, pid)
        win, texts = lookup(app, pid, np, "world")
        click_save(app, win)
        win, warn_texts = popup_win(app, pid, ["此语境已存过"], timeout=10)
        c.screenshot_window(win, "31-anki-context-warning.png")
        before = len(deck_notes())
        assert any("换义项" in t for t in warn_texts), warn_texts
        assert any("仍然保存" in t for t in warn_texts), warn_texts
        spec = app.window(handle=win.hwnd)
        spec.child_window(title="仍然保存", control_type="Button").click_input()
        win, saved_texts = wait_saved(app, pid)
        assert any("此语境之前保存过" in t for t in saved_texts), saved_texts
        after = len(deck_notes())
        assert after == before + 1, (before, after)
        c.screenshot_window(win, "32-anki-saved-anyway.png")
        close(app, pid)
        return {"warning": "shown", "notes_before": before, "notes_after": after}

    step("same-context hint + 仍然保存 (definition without local media)", same_context)

    def same_context_media():
        # Known M2 issue: the popup pre-check fingerprints the raw HTML while the
        # exporter fingerprints the media-stripped HTML, so entries that contain
        # local img/audio skip the warn prompt. Record the observed behaviour.
        win, _ = lookup(app, pid, np, "hello")
        click_save(app, win)
        win, texts = wait_saved(app, pid)
        assert any("此语境之前保存过" in t for t in texts), texts
        warned = any("此语境已存过" in t for t in texts)
        c.screenshot_window(win, "31b-anki-media-context.png")
        close(app, pid)
        return {
            "warn_prompt_shown": warned,
            "observed": "saved directly with exact_context_seen=true (pre-check fingerprint mismatch)",
        }

    step("same-context with local media definition (known bug)", same_context_media)

    def same_word_different_sentences():
        win, _ = lookup(app, pid, np, "hello is the first sentence.", first_word=True)
        click_save(app, win)
        win, _ = wait_saved(app, pid)
        close(app, pid)
        win, _ = lookup(app, pid, np, "hello is the second sentence.", first_word=True)
        click_save(app, win)
        win, saved_texts = wait_saved(app, pid)
        c.screenshot_window(win, "33-anki-same-word-two-sentences.png")
        close(app, pid)
        ids = deck_notes()
        infos = notes_info(ids)
        sentences = [field_value(note, "Sentence") for note in infos]
        assert any("first sentence" in s for s in sentences), sentences
        assert any("second sentence" in s for s in sentences), sentences
        hello_notes = [note for note in infos if field_value(note, "Word") == "hello"]
        return {
            "note_count": len(ids),
            "hello_note_count": len(hello_notes),
            "sentences": sorted(set(sentences)),
        }

    step("same word with two different sentences", same_word_different_sentences)

    def anki_offline():
        import psutil

        for proc in psutil.process_iter(["pid", "name"]):
            if (proc.info["name"] or "").lower() == "anki.exe":
                proc.kill()
        assert c.wait_for(lambda: not anki_up(0.5), timeout=20, interval=0.5), "Anki did not stop"
        win, _ = lookup(app, pid, np, "hello is an offline test.", first_word=True)
        click_save(app, win)
        win, texts = popup_win(app, pid, ["启动 Anki"], timeout=15)
        c.screenshot_window(win, "34-anki-offline.png")
        assert any("ANKI_UNREACHABLE" in t or "Anki 未运行" in t for t in texts), texts
        # Restart Anki and retry from the saveError state.
        assert ensure_anki(120), "Anki did not restart"
        app2 = c.connect(pid)
        spec = app2.window(handle=win.hwnd)
        spec.child_window(title="重试保存", control_type="Button").click_input()
        win, saved_texts = wait_saved(app2, pid, timeout=20)
        c.screenshot_window(win, "35-anki-retry-after-restart.png")
        close(app2, pid)
        return {"offline_hint": "shown", "retry": "saved"}

    step("Anki offline error + retry after restart", anki_offline)

    def missing_field():
        c.kill_app()
        c.set_config_value("word", '"Nope"', "anki.field_map")
        info2 = c.launch_app()
        pid2 = c.app_pid()
        app2 = c.connect(pid2)
        _, np2 = p2.notepad(reset=True)
        p2.popup_hidden(pid2)
        win, _ = lookup(app2, pid2, np2, "hello is a missing field test.", first_word=True)
        click_save(app2, win)
        win, texts = popup_win(app2, pid2, ["缺少字段"], timeout=15)
        assert any("打开 Anki 设置" in t for t in texts), texts
        c.screenshot_window(win, "36-anki-field-missing.png")
        close(app2, pid2)
        # Restore the contract field mapping for any later phase.
        c.kill_app()
        c.set_config_value("word", '"Word"', "anki.field_map")
        c.launch_app()
        return {"error": [t for t in texts if "缺少字段" in t], "startup": info2}

    step("missing Anki field is reported", missing_field)

    # Restore a running app with default mapping for subsequent phases.
    RESULT["final_notes"] = deck_notes()
    RESULT["final_notes_info"] = [
        {
            "noteId": note["noteId"],
            "word": field_value(note, "Word"),
            "sentence": field_value(note, "Sentence"),
        }
        for note in notes_info(RESULT["final_notes"])
    ]
    c.save_json("p3-anki.json", RESULT)
    print("\nP3 Anki: all steps passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

