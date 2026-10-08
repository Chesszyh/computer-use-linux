#!/usr/bin/env node
import { createRequire } from 'node:module';
import { realpathSync, mkdtempSync, writeFileSync } from 'node:fs';
import { dirname, resolve, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { tmpdir } from 'node:os';
import { createInterface } from 'node:readline';

const root = resolve(dirname(realpathSync(fileURLToPath(import.meta.url))), '../../..');
const require = createRequire(import.meta.url);
let cua;
let lines;
let imageDirectory;
let imageNumber = 0;
const send = value => process.stdout.write(`${JSON.stringify(value)}\n`);

try {
  const { createComputerUse } = require(join(root, 'pi/extension/mcp-client.bundle.cjs'));
  cua = await createComputerUse({ binaryPath: join(root, 'target/debug/computer-use-linux') });
  send({ ready: true, backend: 'computer-use-linux-dev', repository: root });
  lines = createInterface({ input: process.stdin, crlfDelay: Infinity });
  for await (const line of lines) {
    if (!line.trim()) continue;
    try {
      const request = JSON.parse(line);
      if (request.close === true) break;
      if (typeof request.tool !== 'string') throw new Error('Supply a tool name and optional arguments.');
      const result = await cua.call(request.tool, request.arguments ?? {});
      for (const item of result.content ?? []) {
        if (item.type !== 'image') continue;
        imageDirectory ??= mkdtempSync(join(tmpdir(), 'cu-dev-images-'));
        const path = join(imageDirectory, `${++imageNumber}.${item.mimeType === 'image/jpeg' ? 'jpg' : 'png'}`);
        writeFileSync(path, Buffer.from(item.data, 'base64'), { mode: 0o600 });
        delete item.data;
        item.path = path;
      }
      send({ ok: true, result });
    } catch (error) {
      send({ ok: false, error: error.message });
    }
  }
} catch (error) {
  send({ ready: false, error: error.message, help: join(root, 'skills/computer-use-linux-dev/references/maintenance.md') });
  process.exitCode = 1;
} finally {
  lines?.close();
  process.stdin.pause();
  await cua?.close();
}
