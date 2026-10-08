---
name: computer-use-linux-dev
description: Extend computer-use-linux on this machine when desktop work needs stable UI diffs, exact text selection, HTML paste, background Hyprland capture/input, batches, waits, application launch, or persistent application bindings. Start the local CU dev branch on demand; use the upstream skill for ordinary desktop operations.
---

# Computer Use Linux — local dev extension

Use this alongside the installed `computer-use-linux` skill. The base skill and
default MCP server follow the upstream npm package. This extension uses the source-built
implementation when its additional capabilities materially
help the current task. Automatic selection is appropriate; the user need not
name this skill explicitly.

## Choose the backend

Use the upstream tools for ordinary screenshots, window discovery, keyboard input
and semantic clicks/value setting. Use dev when the task needs:

- Incremental accessibility observations with stable element identities.
- Selecting a text occurrence, placing its caret, or inserting into its selection
  through AT-SPI (Linux accessibility interfaces), without focusing the window.
- HTML paste with restoration of previous clipboard formats.
- Capturing an occluded Hyprland window, or sending a background shortcut to a
  native Wayland window.
- Installed application launch, deterministic batches, condition waits, session
  reset, or the persistent JavaScript application API.

Prefer an equivalent upstream capability once the loaded upstream tool schema
supports it. Do not switch to dev merely because an unrelated operation failed.

## Start an on-demand session

The reusable bridge keeps one dev MCP process alive, accepts one JSON request per
line, and serializes operations. From a terminal tool that supports persistent
stdin, start it from the repository root with `tty: true`:

```sh
node skills/computer-use-linux-dev/scripts/session.mjs
```

Wait for `ready: true`. Send newline-terminated requests through that terminal's
stdin. First discover the intended window, then observe it before using elements:

```json
{"tool":"list_windows"}
{"tool":"get_app_state","arguments":{"window_id":123,"include_screenshot":false,"state_mode":"full"}}
```

Replace `123` with the returned window id. Responses have `ok` and `result`;
inspect `result.structuredContent` and MCP `isError`/action `ok` fields. Images are
saved to private temporary files and returned as absolute `path` entries: use the
host's image-viewing tool to inspect those files. Close the session when the
workflow finishes:

```json
{"close":true}
```

Keep all dependent observations/actions in this dev session. Upstream element
indices and dev indices are not interchangeable. After switching back, obtain a
fresh upstream observation. Do not send input from both backends concurrently.
After an ambiguous failure, observe the app before deciding whether to retry.

## Use the additional operations

Read [Desktop interaction](../../docs/desktop-control.md) for tool parameters and
JavaScript examples; it is the canonical API reference. In particular:

- Semantic selection does not give an editor keyboard focus; focus its field
  before `paste`, then verify the resulting document.
- Background screenshots use `coordinate_space="window_surface"`; use that
  origin for relative click/scroll and divide preview pixels by `scale`.
- XWayland background keys remain unsupported. Coordinate mouse actions use the
  shared desktop pointer; dev does not provide locked-session operation.
- A failed batch leaves earlier actions applied. Inspect `wait_satisfied` after
  waiting; `reset_session` invalidates old element identities.

## Build and maintenance

The bridge resolves the repository through its real file location and runs
`target/debug/computer-use-linux` with the matching bundled JavaScript client.
It checks their tool catalog through the existing client handshake. It does not
change the default MCP configuration or install npm packages when invoked.

If the build is missing or the catalog mismatches, read
[Local build maintenance](references/maintenance.md). If installing this skill separately, use a symlink to its repository directory
so the bridge can locate the build.
