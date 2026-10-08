#!/usr/bin/env python3
"""Exercise semantic editing, background capture/input and clipboard preservation on a live desktop."""
import argparse
import base64
import json
import os
from pathlib import Path
import subprocess
import sys
import time

from mcp_safety_check import McpClient


def fixture():
    import gi
    gi.require_version("Gtk", "3.0")
    from gi.repository import Gtk, Gdk, GLib, Atk
    GLib.set_prgname("computer-use-parity-test")
    GLib.set_application_name("Computer Use Parity Test")
    Atk.get_root().set_name("Computer Use Parity Test")
    target = Gtk.Window(title="CU Parity Target")
    target.set_default_size(600, 320)
    box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=16)
    box.set_border_width(25)
    target.add(box)
    entry = Gtk.Entry()
    entry.get_accessible().set_name("Parity entry")
    entry.set_text("initial")
    box.pack_start(entry, False, False, 0)
    label = Gtk.Label(label="Ready")
    box.pack_start(label, False, False, 0)
    button = Gtk.Button(label="Change state")
    button.connect("clicked", lambda *_: label.set_text("Button activated"))
    entry.connect("activate", lambda *_: label.set_text("Return received"))
    box.pack_start(button, False, False, 0)
    html_label = Gtk.Label(label="HTML not received")
    box.pack_start(html_label, False, False, 0)
    def keypress(_, event):
        if event.state & Gdk.ModifierType.CONTROL_MASK and event.keyval in (Gdk.KEY_v, Gdk.KEY_V):
            clipboard = Gtk.Clipboard.get(Gdk.SELECTION_CLIPBOARD)
            def received(_, data, __):
                if data.get_data():
                    html_label.set_text("HTML received: " + data.get_data().decode())
            clipboard.request_contents(Gdk.Atom.intern("text/html", False), received, None)
        return False
    target.connect("key-press-event", keypress)
    target.connect("destroy", Gtk.main_quit)
    target.show_all()
    entry.grab_focus()
    cover = Gtk.Window(title="CU Parity Cover")
    cover.set_default_size(640, 380)
    cover.add(Gtk.Label(label="COVER WINDOW — capture must still show the target"))
    cover.show_all()
    cover.present()
    print("ready", flush=True)
    Gtk.main()


def data(result):
    if result.get("isError"):
        raise AssertionError(result)
    if result.get("structuredContent") is not None:
        return result["structuredContent"]
    for item in result.get("content", []):
        if item.get("type") == "text":
            try:
                return json.loads(item["text"])
            except json.JSONDecodeError:
                pass
    raise AssertionError("No structured result")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", default="target/debug/computer-use-linux")
    parser.add_argument("--fixture", action="store_true")
    parser.add_argument("--launch-probe", action="store_true")
    parser.add_argument("--backend", choices=["wayland","x11"], default="wayland")
    parser.add_argument("--output", default="/tmp/computer-use-parity")
    args = parser.parse_args()
    if args.launch_probe:
        import gi
        gi.require_version("Gtk", "3.0")
        from gi.repository import Gtk, GLib
        window = Gtk.Window(title="CU Launch Probe")
        window.add(Gtk.Label(label="Launched through its desktop entry"))
        window.show_all()
        GLib.timeout_add_seconds(4, lambda: (Gtk.main_quit(), False)[1])
        Gtk.main()
        return
    if args.fixture:
        fixture()
        return
    binary = Path(args.binary).resolve()
    output = Path(args.output)
    output.mkdir(parents=True, exist_ok=True)
    app = subprocess.Popen(["/usr/bin/python3", __file__, "--fixture"], stdout=subprocess.PIPE, text=True, env={**os.environ, "GDK_BACKEND":args.backend})
    data_home = output / "xdg"
    entries = data_home / "applications"
    entries.mkdir(parents=True, exist_ok=True)
    (entries / "cu-launch-probe.desktop").write_text(f'[Desktop Entry]\nType=Application\nName=CU launch probe\nExec=/usr/bin/python3 "{Path(__file__).resolve()}" --launch-probe\nTerminal=false\n')
    client = McpClient(binary, {"XDG_DATA_HOME": str(data_home)})
    def call(name, arguments=None):
        return client.request("tools/call", {"name": name, "arguments": arguments or {}})["result"]
    def action(name, arguments):
        result = data(call(name, arguments))
        assert result["ok"], result
        return result
    def state(**extra):
        return data(call("get_app_state", {"pid": app.pid, "include_screenshot": False, **extra}))
    evidence = {}
    try:
        assert app.stdout.readline().strip() == "ready"
        client.request("initialize", {"protocolVersion":"2024-11-05", "capabilities":{}, "clientInfo":{"name":"desktop-parity-test","version":"1"}})
        client.notify("notifications/initialized")
        initial = state()
        assert not initial["accessibility_error"], initial.get("accessibility_error")
        evidence["readiness"] = data(call("doctor"))["readiness"]
        windows = data(call("list_windows"))["windows"]
        target = next(w for w in windows if w.get("title") == "CU Parity Target")
        cover = next(w for w in windows if w.get("title") == "CU Parity Cover")
        target_id = target["window_id"]
        monitors = json.loads(subprocess.check_output(["hyprctl","monitors","-j"]))
        monitor = next(m for m in monitors if m.get("focused"))
        origin_x, origin_y = monitor["x"] + 150, monitor["y"] + 150
        for window, width, height in [(target,600,320),(cover,680,400)]:
            address = f'address:0x{window["window_id"]:x}'
            for dispatch in [f'hl.dsp.window.float({{window="{address}",action="set"}})', f'hl.dsp.window.resize({{window="{address}",x={width},y={height},relative=false}})', f'hl.dsp.window.move({{window="{address}",x={origin_x},y={origin_y},relative=false}})']:
                result = subprocess.run(["hyprctl","dispatch",dispatch],capture_output=True,text=True,check=True)
                assert result.stdout.strip() == "ok", result.stdout
        time.sleep(.3)
        entry = next(n for n in initial["accessibility_tree"] if n.get("name") == "Parity entry")
        index = entry["index"]
        action("set_value", {"element_index": index, "value": "甲🙂 one one"})
        action("select_text", {"element_index": index, "text":"one", "prefix":"one "})
        action("type_text", {"element_index": index, "text":"中文🙂"})
        changed = state(state_mode="diff")
        result_entry = next(n for n in changed["accessibility_tree"] if n["index"] == index)
        assert result_entry["text"]["content"] == "甲🙂 one 中文🙂", result_entry
        evidence["unicode_selection_and_insert"] = True
        assert changed["tree_changes"]["mode"] == "diff"
        unchanged = state(state_mode="diff")
        for _ in range(3):
            if not unchanged["accessibility_tree"]: break
            time.sleep(.2)
            unchanged = state(state_mode="diff")
        assert unchanged["accessibility_tree"] == [], unchanged["tree_changes"]
        evidence["unchanged_diff_nodes"] = 0
        action("select_text", {"element_index":index, "text":"中文🙂", "selection_type":"cursor_before"})
        action("type_text", {"element_index":index, "text":"前"})
        assert next(n for n in state()["accessibility_tree"] if n["index"] == index)["text"]["content"] == "甲🙂 one 前中文🙂"
        evidence["caret_placement"] = True
        action("click", {"window_id":target_id,"x":100,"y":38,"relative":True,"coordinate_space":"window_surface"})
        action("activate_window", {"window_id": cover["window_id"]})
        action("click", {"window_id":cover["window_id"],"x":50,"y":50,"relative":True})
        before = data(call("focused_window"))
        key_result = data(call("press_key", {"window_id": target_id, "key":"Return", "background":True}))
        after = data(call("focused_window"))
        assert before["focused_window"]["window_id"] == after["focused_window"]["window_id"], (before, after)
        if args.backend == "wayland":
            assert key_result["ok"], key_result
            assert any(n.get("name") == "Return received" for n in state()["accessibility_tree"])
            evidence["background_key_preserves_focus"] = True
        else:
            assert not key_result["ok"] and "unsupported" in key_result["message"], key_result
            evidence["background_key"] = "unsupported_for_xwayland"
            action("press_key", {"window_id": target_id, "key":"Return"})
            assert any(n.get("name") == "Return received" for n in state()["accessibility_tree"])
            evidence["foreground_key"] = True
            action("click", {"window_id":cover["window_id"],"x":50,"y":50,"relative":True})
            after = data(call("focused_window"))
        shot = call("get_app_state", {"window_id":target_id, "background":True})
        shot_state = data(shot)
        assert not shot_state["screenshot_error"], shot_state["screenshot_error"]
        assert shot_state["screenshot"]["source"] == "hyprland-toplevel-export"
        image = next(c for c in shot["content"] if c["type"] == "image")
        (output / "background-window.png").write_bytes(base64.b64decode(image["data"]))
        assert data(call("focused_window"))["focused_window"]["window_id"] == after["focused_window"]["window_id"]
        evidence["background_capture_preserves_focus"] = True
        background_shot = call("screenshot", {"window_id":target_id, "background":True})
        assert not background_shot.get("isError"), background_shot
        assert any(c["type"] == "image" for c in background_shot["content"])
        assert data(call("focused_window"))["focused_window"]["window_id"] == after["focused_window"]["window_id"]
        evidence["standalone_background_capture_preserves_focus"] = True
        click_result = action("click", {"window_id":target_id,"x":300,"y":120,"relative":True,"coordinate_space":"window_surface"})
        assert any(n.get("name") == "Button activated" for n in state()["accessibility_tree"]), {"click":click_result, "focused": data(call("focused_window")), "cursor": subprocess.check_output(["hyprctl","cursorpos","-j"], text=True)}
        evidence["surface_coordinate_click"] = True
        # The test never prints or stores the user's existing clipboard contents.
        clipboard_types = subprocess.run(["wl-paste","--list-types"], capture_output=True, text=True).stdout.splitlines()
        clipboard_before = [(mime, subprocess.run(["wl-paste","--no-newline","--type",mime],capture_output=True,check=True).stdout) for mime in clipboard_types]
        action("click", {"window_id":target_id,"x":100,"y":38,"relative":True,"coordinate_space":"window_surface"})
        action("select_text", {"element_index":index,"text":"甲🙂 one 前中文🙂"})
        action("paste", {"window_id":target_id,"text":"Pasted 中文","html":"<b>Pasted 中文</b>"})
        pasted = state()
        assert next(n for n in pasted["accessibility_tree"] if n["index"] == index)["text"]["content"] == "Pasted 中文"
        assert any(n.get("name") == "HTML received: <b>Pasted 中文</b>" for n in pasted["accessibility_tree"]), "HTML clipboard format did not reach the app"
        for mime, expected in clipboard_before:
            actual = subprocess.run(["wl-paste","--no-newline","--type",mime],capture_output=True,check=True).stdout
            assert actual == expected, f"Clipboard format {mime} was not restored"
        evidence["html_paste_and_clipboard_restore"] = True
        button = next(n for n in pasted["accessibility_tree"] if n.get("name") == "Change state" and n.get("actions"))
        batch = data(call("perform_actions", {"actions":[{"tool":"click","element_index":button["index"]}], "observe":{"pid":app.pid,"include_screenshot":False}}))
        assert batch["batch"]["ok"]
        waited = data(call("wait_for", {"pid":app.pid,"name":"Button activated","timeout_ms":1500}))
        assert waited["wait_satisfied"]
        missing = data(call("wait_for", {"pid":app.pid,"name":"never appears","timeout_ms":50}))
        assert not missing["wait_satisfied"]
        evidence["batch_and_wait"] = True
        failed = data(call("perform_actions", {"actions":[{"tool":"set_value","element_index":index,"value":"batch stopped"},{"tool":"select_text","element_index":index,"text":"missing"},{"tool":"type_text","element_index":index,"text":"must not run"}], "observe":{"pid":app.pid,"include_screenshot":False}}))
        assert not failed["batch"]["ok"] and failed["batch"]["completed_actions"] == 1 and failed["batch"]["attempted_actions"] == 2
        assert next(n for n in failed["observation"]["accessibility_tree"] if n["index"] == index)["text"]["content"] == "batch stopped"
        evidence["batch_stops_on_failure"] = True
        subprocess.run(["node", str(Path(__file__).with_name("cua_smoke.mjs")), str(binary), str(target_id), str(cover["window_id"])], check=True)
        evidence["javascript_app_api"] = True
        action("reset_session", {})
        state()
        stale = data(call("set_value", {"element_index":index,"value":"stale"}))
        assert not stale["ok"]
        evidence["reset_invalidates_elements"] = True
        apps = data(call("list_launchable_apps"))["apps"]
        assert any(a["id"] == "cu-launch-probe.desktop" for a in apps)
        action("launch_app", {"app_id":"cu-launch-probe.desktop"})
        deadline = time.monotonic() + 3
        launched = False
        while time.monotonic() < deadline:
            if any(w.get("title") == "CU Launch Probe" for w in data(call("list_windows"))["windows"]):
                launched = True
                break
            time.sleep(.15)
        assert launched, "The desktop entry did not open its window"
        evidence["application_launch"] = True
        (output / "results.json").write_text(json.dumps(evidence, indent=2))
        print(json.dumps(evidence, indent=2))
    finally:
        client.close()
        app.terminate()
        app.wait(timeout=5)


if __name__ == "__main__":
    main()
