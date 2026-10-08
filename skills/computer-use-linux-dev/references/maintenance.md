# Local build maintenance

Run these commands from your computer-use-linux checkout. Inspect its current
branch and working tree before building. Do not discard local changes or switch
branches automatically during a desktop task.

From this repository, build the binary and matching client together:

```sh
cargo build --locked
python3 scripts/generate_pi_tool_catalog.py --binary target/debug/computer-use-linux
npm ci --prefix pi --ignore-scripts --no-audit --no-fund
npm run build --prefix pi
python3 scripts/mcp_safety_check.py --binary target/debug/computer-use-linux
```

Restart a dev bridge after rebuilding. Its existing process retains the old
binary/client. The source checkout and generated files are the dev runtime;
updating the upstream npm package does not update them.

For agent-authored JavaScript workflows, import `createComputerUse` from
`pi/extension/mcp-client.bundle.cjs` and pass the absolute source-built binary as
`binaryPath`. Keep one client for the workflow and close it in `finally`. See
[the application API](../../../docs/desktop-control.md#javascript-application-bindings).
