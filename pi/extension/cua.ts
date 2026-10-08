import { ComputerUseMcpClient } from "./mcp-client.js";
import { GENERATED_SERVER_VERSION, GENERATED_TOOL_CATALOG_HASH } from "./generated-tools.js";

type Args = Record<string, unknown>;
type Node = { index: number; object_ref: string; role: string; name?: string; states: string[]; text?: { content?: string } };
type Window = { window_id: number; title?: string; app_id?: string; wm_class?: string; pid?: number; bounds?: { x?: number; y?: number; width: number; height: number } };
type State = { accessibility_tree: Node[]; tree_changes: { mode: string; removed: number[] }; accessibility_error?: string; screenshot_error?: string };
type Image = { type: "image"; data: string; mimeType: string; width?: number; height?: number; scale?: number; coordinate_width?: number; coordinate_height?: number; coordinate_space?: string };

function unpack(result: any): any {
    if (result.isError) throw new Error(result.content.filter((c: any) => c.type === "text").map((c: any) => c.text).join("\n"));
    const value = result.structuredContent ?? result.content.filter((c: any) => c.type === "text").map((c: any) => { try { return JSON.parse(c.text); } catch { return undefined; } }).find(Boolean);
    if (value?.ok === false) throw new Error(value.message ?? value.error ?? "Desktop action failed");
    return value;
}

function render(state: State): string {
    const lines = state.accessibility_tree.map(node => `[${node.index}] ${node.role} ${JSON.stringify(node.name ?? "")} ${node.states.join(" ")}${node.text?.content ? ` text=${JSON.stringify(node.text.content)}` : ""}`);
    if (state.tree_changes.mode === "diff") lines.unshift(`Changes; removed: ${state.tree_changes.removed.join(", ") || "none"}`);
    return lines.join("\n") || "No accessibility changes.";
}

export class ComputerUse {
    private readonly client: ComputerUseMcpClient;
    private pending: Promise<unknown> = Promise.resolve();
    private generation = 0;

    constructor(options: { binaryPath?: string; env?: Record<string, string> } = {}) {
        this.client = new ComputerUseMcpClient({
            binaryPath: options.binaryPath ?? "computer-use-linux", clientVersion: GENERATED_SERVER_VERSION,
            env: { ...Object.fromEntries(Object.entries(process.env).filter((entry): entry is [string, string] => entry[1] !== undefined)), ...options.env },
            expectedCatalogHash: GENERATED_TOOL_CATALOG_HASH, expectedServerVersion: GENERATED_SERVER_VERSION,
        });
    }

    async call(name: string, args: Args = {}): Promise<any> {
        const request = this.pending.then(() => this.client.callTool(name, args));
        this.pending = request.catch(() => {});
        const result = await request;
        unpack(result);
        return result;
    }

    async listWindows(): Promise<Window[]> { return unpack(await this.call("list_windows")).windows; }
    async listApps(): Promise<Array<{ id: string; name: string }>> { return unpack(await this.call("list_launchable_apps")).apps; }
    async launchApp(id: string): Promise<void> { await this.call("launch_app", { app_id: id }); }

    async getApp(target: string | { windowId: number }): Promise<ComputerApp> {
        const windows = await this.listWindows();
        const matches = windows.filter(window => typeof target === "string"
            ? [window.app_id, window.wm_class, window.title].some(value => value?.toLowerCase() === target.toLowerCase())
            : window.window_id === target.windowId);
        if (matches.length !== 1) throw new Error(`Expected one open window; found ${matches.length}. Use listWindows() and getApp({windowId}).`);
        const app = new ComputerApp(this, matches[0], this.generation);
        await app.getAXState({ disableDiffing: true });
        return app;
    }

    async reset(): Promise<void> { await this.call("reset_session"); this.generation++; }
    assertGeneration(generation: number): void { if (generation !== this.generation) throw new Error("Application binding expired after reset; call getApp again."); }
    async close(): Promise<void> { await this.client.close(); this.generation++; }
}

export class ComputerApp {
    private nodes = new Map<number, Node>();
    private coordinateSpace = "desktop_crop";
    constructor(private readonly owner: ComputerUse, readonly window: Window, private readonly generation: number) {}

    private check(): void { this.owner.assertGeneration(this.generation); }
    private element(index: number): Args {
        this.check();
        const node = this.nodes.get(index);
        if (!node) throw new Error(`Element ${index} is not in this app's observation. Call getAXState().`);
        return { element_index: index, element_identifier: node.object_ref };
    }
    private async observe(options: Args): Promise<{ state: State; images: Image[] }> {
        this.check();
        const result = await this.owner.call("get_app_state", { ...options, window_id: this.window.window_id });
        const state = unpack(result) as State;
        if (state.accessibility_error) throw new Error(state.accessibility_error);
        if (state.screenshot_error) throw new Error(state.screenshot_error);
        if (state.tree_changes.mode === "full") this.nodes.clear();
        for (const index of state.tree_changes.removed) this.nodes.delete(index);
        for (const node of state.accessibility_tree) this.nodes.set(node.index, node);
        if (result.content.some((item: any) => item.type === "image")) this.coordinateSpace = options.background ? "window_surface" : "desktop_crop";
        return { state, images: result.content.filter((item: any) => item.type === "image").map((item: any) => ({ ...item, ...unpack(result).screenshot })) };
    }
    async getAXState(options: { disableDiffing?: boolean } = {}): Promise<string> {
        return render((await this.observe({ include_screenshot: false, state_mode: options.disableDiffing ? "full" : "diff" })).state);
    }
    async getAXStateAndScreenshot(options: { background?: boolean } = {}): Promise<{ state: string; screenshots: Image[] }> {
        const result = await this.observe({ include_screenshot: true, state_mode: "full", ...options });
        return { state: render(result.state), screenshots: result.images };
    }
    async getScreenshot(options: { background?: boolean } = {}): Promise<Image> {
        this.check();
        const result = await this.owner.call("screenshot", { ...options, window_id: this.window.window_id });
        this.coordinateSpace = options.background ? "window_surface" : "desktop_crop";
        return { ...result.content.find((item: any) => item.type === "image"), ...unpack(result) };
    }
    async click(target: number | [number, number], options: { button?: string; click_count?: number } = {}): Promise<void> {
        this.check();
        const args = typeof target === "number" ? { element_index: this.element(target).element_index } : { x: target[0], y: target[1], relative: true, coordinate_space: this.coordinateSpace, window_id: this.window.window_id };
        await this.owner.call("click", { ...args, ...options });
    }
    async setValue(index: number, value: string): Promise<void> { await this.owner.call("set_value", { ...this.element(index), value }); }
    async selectText(index: number, text: string, options: { prefix?: string; suffix?: string; selectionType?: "text" | "cursor_before" | "cursor_after" } = {}): Promise<void> {
        await this.owner.call("select_text", { ...this.element(index), text, prefix: options.prefix, suffix: options.suffix, selection_type: options.selectionType });
    }
    async performSecondaryAction(index: number, action: string): Promise<void> { await this.owner.call("perform_action", { ...this.element(index), action }); }
    async typeText(text: string, index?: number): Promise<void> {
        this.check();
        await this.owner.call("type_text", { text, ...(index === undefined ? { window_id: this.window.window_id } : this.element(index)) });
    }
    async pressKey(key: string, options: { background?: boolean } = {}): Promise<void> { this.check(); await this.owner.call("press_key", { ...options, key, window_id: this.window.window_id }); }
    async paste(text: string, options: { html?: string } = {}): Promise<void> { this.check(); await this.owner.call("paste", { text, ...options, window_id: this.window.window_id }); }
    async scroll(target: number | [number, number], direction: "up" | "down" | "left" | "right", pages = 1): Promise<void> {
        this.check();
        const args = typeof target === "number" ? { element_index: this.element(target).element_index } : { x: target[0], y: target[1], relative: true, coordinate_space: this.coordinateSpace };
        await this.owner.call("scroll", { ...args, direction, pages, window_id: this.window.window_id });
    }
    async drag(from: [number, number], to: [number, number]): Promise<void> {
        this.check();
        await this.owner.call("activate_window", { window_id: this.window.window_id });
        const bounds = (await this.owner.listWindows()).find(window => window.window_id === this.window.window_id)?.bounds;
        if (bounds?.x === undefined || bounds.y === undefined) throw new Error("The target window has no usable coordinate bounds.");
        const originX = this.coordinateSpace === "window_surface" ? bounds.x : Math.max(0, bounds.x);
        const originY = this.coordinateSpace === "window_surface" ? bounds.y : Math.max(0, bounds.y);
        await this.owner.call("drag", { start_x: originX + from[0], start_y: originY + from[1], end_x: originX + to[0], end_y: originY + to[1] });
    }
    async waitFor(selector: { role?: string; name?: string; text?: string; states?: string[] }, timeoutMs = 5000): Promise<boolean> {
        this.check();
        const result = unpack(await this.owner.call("wait_for", { ...selector, window_id: this.window.window_id, timeout_ms: timeoutMs }));
        return result.wait_satisfied;
    }
}

export async function createComputerUse(options: { binaryPath?: string; env?: Record<string, string> } = {}): Promise<ComputerUse> {
    const client = new ComputerUse(options);
    try { await client.call("doctor"); return client; }
    catch (error) { await client.close(); throw error; }
}
