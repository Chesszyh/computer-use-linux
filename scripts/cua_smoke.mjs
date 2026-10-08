import assert from "node:assert/strict";
import { createRequire } from "node:module";
const require = createRequire(import.meta.url);
const { createComputerUse } = require("../pi/extension/mcp-client.bundle.cjs");
async function main() {
const cua = await createComputerUse({ binaryPath: process.argv[2] });
try {
    const app = await cua.getApp({ windowId: Number(process.argv[3]) });
    const state = await app.getAXState({ disableDiffing: true });
    const index = Number(state.match(/\[(\d+)\][^\n]*"Parity entry"/)[1]);
    await app.setValue(index, "first second");
    await app.selectText(index, "second");
    await app.typeText("中文", index);
    await cua.getApp({ windowId: Number(process.argv[4]) });
    await app.selectText(index, "中文", { selectionType: "cursor_after" });
    await app.typeText("🙂", index);
    assert.match(await app.getAXState(), /first 中文🙂/);
    assert.match(await app.getAXState(), /Changes; removed: none/);
    const shot = await app.getScreenshot({ background: true });
    assert.equal(shot.type, "image");
    assert.ok(shot.data.length > 100);
    await app.setValue(index, "drag selection sample");
    await app.drag([35, 38], [240, 38]);
    const observed = await cua.call("get_app_state", { window_id: app.window.window_id, include_screenshot: false });
    const field = observed.structuredContent.accessibility_tree.find(node => node.name === "Parity entry");
    assert.ok(field.text.selections.some(range => range.end_offset > range.start_offset), "Drag did not select text");
    await cua.reset();
    await assert.rejects(() => app.typeText("expired", index), /expired/);
    console.log("CUA JavaScript API: binding, editing, interleaving, screenshots and reset passed.");
} finally { await cua.close(); }

}
main().catch(error => { console.error(error.message); process.exitCode = 1; });
