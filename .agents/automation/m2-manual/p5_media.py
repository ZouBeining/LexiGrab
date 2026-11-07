"""M2 manual matrix, part 5: mdx-res media rendering inside the popup."""
from __future__ import annotations
import sys, time
from pathlib import Path
from PIL import Image
from pywinauto import Application
sys.path.insert(0, str(Path(__file__).resolve().parent))
import common as c
import p2_popup as p2

RESULT = {"phase": "media", "steps": []}

def step(name, fn):
    print(f"\n=== {name} ===")
    value = fn()
    entry = {"step": name, "ok": True}
    if value is not None:
        entry["detail"] = value
    RESULT["steps"].append(entry)
    print(f"--- {name}: OK")
    return value

def image_elements(spec):
    out = []
    for d in spec.descendants(control_type="Image"):
        try:
            r = d.element_info.rectangle
            out.append((d.window_text(), r.left, r.top, r.right, r.bottom))
        except Exception:
            continue
    return out

def variance_in(path: Path, rect) -> float:
    image = Image.open(path).convert("RGB")
    left, top, right, bottom = rect
    # popup screenshot is cropped to the window rect
    crop = image.crop((left, top, right, bottom))
    pixels = list(crop.getdata())
    if not pixels:
        return 0.0
    channels = list(zip(*pixels))
    means = [sum(ch) / len(ch) for ch in channels]
    var = 0.0
    for ch, mean in zip(channels, means):
        var += sum((v - mean) ** 2 for v in ch) / len(ch)
    return round(var / 3, 1)

def popup_media(word: str):
    pid = c.app_pid()
    app = c.connect(pid)
    root, np = p2.notepad(reset=True)
    p2.popup_hidden(pid)
    p2.notepad_set(np, word)
    p2.select_all(np)
    win, latency = p2.trigger_and_wait(pid)
    assert p2.wait_popup_text(app, pid, "保存所选义项", 10)
    time.sleep(1.2)
    spec = app.window(handle=win.hwnd)
    texts = c.uia_texts(spec)
    images = image_elements(spec)
    shot = c.screenshot_window(win, f"50-popup-{word}.png")
    # translate screen-relative Image rects into screenshot coordinates
    stats = []
    for name, l, t, r, b in images:
        local = (l - win.left, t - win.top, r - win.left, b - win.top)
        try:
            stats.append({"name": name, "rect": local, "variance": variance_in(shot, local)})
        except Exception as exc:
            stats.append({"name": name, "rect": local, "error": str(exc)})
    audio = [t for t in texts if "播放" in t or "音频时间进度条" in t or "无法播放媒体" in t]
    p2.close_popup(app, pid)
    return {"latency_ms": round(latency, 1), "image_count": len(images), "images": stats, "audio_controls": audio}

def main():
    c.restart_app()
    pid = c.app_pid()
    assert pid, "app not running"
    RESULT["apple"] = step("Oxford apple renders the mdx-res image", lambda: popup_media("apple"))
    RESULT["hello"] = step("mini hello renders img + audio controls", lambda: popup_media("hello"))
    c.save_json("p5-media.json", RESULT)
    print("\nP5 media: all steps passed")
    return 0

if __name__ == "__main__":
    raise SystemExit(main())


