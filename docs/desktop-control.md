# Desktop interaction

Use the MCP server for desktop control. A persistent JavaScript client is also
available from `@agent-sh/computer-use-linux/cua` on Node.js 22.19 or newer.
Browser tabs remain the responsibility of a browser-specific integration.

## Observe changes

`get_app_state` returns a full accessibility tree by default. Pass
`state_mode: "diff"` to request changes since the same target was last observed.
The first observation returns a full tree. Subsequent responses contain added
and changed nodes in `accessibility_tree` and removed element indices in
`tree_changes.removed`. An empty diff means the captured tree did not change.

Element indices retain their identity across observations and application
switches within the same MCP connection. Semantic selectors refer to the most
recent observation. Re-observe after actions before choosing the next target.
Truncated or failed observations discard that target's diff baseline, so the
next response is full. `reset_session` invalidates all cached elements and
releases portal/input connections without closing application windows.

The server waits briefly after input before observing. For a specific completion
condition, use `wait_for` with an app/window target and `role`, `name`, `text`, or
`states`. It returns a full observation and `wait_satisfied`. A timeout returns
`wait_satisfied: false`; it does not indicate that the requested state appeared.

## Edit text by meaning

`set_value` replaces an editable element's contents or sets its numeric value.
`select_text` selects an exact occurrence, with optional immediately preceding
`prefix` or following `suffix` to distinguish repeated matches. Set
`selection_type` to `cursor_before` or `cursor_after` to place the caret instead.
The tool reads the selection back before reporting success.

`type_text` with `element_index` or `element_identifier` inserts at that element's
caret, replacing its selected text. These AT-SPI operations do not activate the
window. The target must implement the relevant Text/EditableText interfaces.
Without an element target, `type_text` retains its normal foreground keyboard
behavior.

```json
{"element_index": 12, "text": "old name", "prefix": "Owner: "}
```

Call `select_text` with the above arguments, then `type_text` with
`{"element_index":12,"text":"new name"}` and observe the result.

## Paste formatted content

`paste` takes `text`, optional `html`, and the usual window selectors. First
focus the destination editor inside that window; selecting text through AT-SPI
alone does not necessarily give the editor keyboard focus. It offers
both plain text and HTML when HTML is supplied, sends the application's paste
shortcut, then restores the previous Wayland clipboard formats. Terminal windows
use their configured paste shortcut (Ctrl+Shift+V or Shift+Insert). If another application takes the clipboard during the operation,
its new contents are retained. The restored clipboard owner outlives the MCP
connection and exits when another application copies something.

```json
{"window_id":123,"text":"Project status","html":"<h2>Project status</h2>"}
```

The compositor must support Wayland data-control. The receiving application
chooses the format it accepts. Paste delivery does not prove that the desired
document content was inserted; observe the destination afterwards.

## Work with background windows

On Hyprland, `screenshot` and `get_app_state` accept `background: true` with a
window target. They capture the window surface through the compositor without
activating it. The result reports `source: "hyprland-toplevel-export"` and
`coordinate_space: "window_surface"`; it excludes server-side decorations.
For coordinate `click`/`scroll` from this image, use `relative: true` and
`coordinate_space: "window_surface"`, dividing preview pixels by `scale`.
The default screenshot path continues to capture/crop the visible desktop.

`press_key` accepts `background: true` with a native Wayland window on Hyprland.
X11/XWayland background key delivery is unsupported; use element-targeted AT-SPI
operations or foreground keys for those windows. It sends
the chord to that window without changing desktop focus. Applications may still
open dialogs or focus other windows in response. On other desktops this mode
reports unsupported instead of switching to foreground input.

Element `click`, `perform_action`, `set_value`, `select_text`, and element-targeted
`type_text` can use AT-SPI without activating a window. Coordinate mouse actions
continue to use the shared desktop pointer. Background capture and semantic
input do not provide a separate desktop or automatic session unlock.

## Discover and start applications

`list_launchable_apps` lists visible XDG desktop entries using their stable ids.
`launch_app` accepts one of those ids and delegates to the desktop's launcher.
Follow it with `list_windows` to select the intended window. `list_apps` continues
to report running processes and accessibility application roots.

## Combine deterministic actions

`perform_actions` executes the supplied actions in order and stops on the first
failure. Each entry uses `tool` to name its operation. `observe` requests a fresh
state after the sequence, including when an action failed.

```json
{
  "actions": [
    {"tool":"set_value","element_index":12,"value":"Report"},
    {"tool":"click","element_index":14}
  ],
  "observe":{"window_id":123,"include_screenshot":false,"state_mode":"diff"}
}
```

The result contains `batch` and, when requested, `observation`. The batch is not
a transaction: earlier successful actions remain applied if a later action
fails. Keep dependent UI discovery between batches.

## JavaScript application bindings

The client reuses one MCP connection, serializes its requests and keeps each
application's element map. `getApp` requires one matching open window. Use the
exact window id when an application has multiple windows.

```js
const { createComputerUse } = require('@agent-sh/computer-use-linux/cua');
const cua = await createComputerUse();
try {
  const windows = await cua.listWindows();
  const chosen = windows.find(window => window.title === 'Project notes');
  const app = await cua.getApp({ windowId: chosen.window_id });
  console.log(await app.getAXState({ disableDiffing: true }));
  // Select the element index from the returned state.
  await app.selectText(12, 'old name');
  await app.typeText('new name', 12);
  console.log(await app.getAXState());
} finally {
  await cua.close();
}
```

App bindings expose `getAXState`, `getAXStateAndScreenshot`, `getScreenshot`,
`click`, `setValue`, `selectText`, `performSecondaryAction`, `typeText`,
`pressKey`, `paste`, `scroll`, `drag`, and `waitFor`. Screenshots are MCP image objects
with `data` (base64) and `mimeType`; they are not printed automatically.
`cua.reset()` expires existing bindings. `cua.close()` closes the MCP connection.
Neither closes user applications.

For a source build, pass `binaryPath` to `createComputerUse`, regenerate the Pi
tool catalog and rebuild the bundled client after changing tools:

```bash
cargo build --locked
python3 scripts/generate_pi_tool_catalog.py --binary target/debug/computer-use-linux
npm ci --prefix pi
npm run build --prefix pi
```

## Live verification

`/usr/bin/python3 scripts/desktop_parity_test.py` launches disposable GTK windows
and verifies Unicode editing, observation diffs, background input/capture,
formatted clipboard delivery/restoration, batches, waiting and JavaScript
bindings. It changes desktop focus and clipboard ownership during the test.
It requires Hyprland, Python GTK3 bindings, `wl-paste`, Node.js and a source-built
binary/client. Use `--binary` and `--output` to select the binary and evidence
directory. The evidence contains the fixture screenshot and pass results; it
does not contain the user's clipboard data.
