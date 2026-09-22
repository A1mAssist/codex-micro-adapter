// Copies the ChatGPT app's own webview bundle out of the installed app, so the
// desktop shell can load the real settings UI instead of a lookalike.
//
// The bundle is OpenAI's code, so it is deliberately NOT in this repository: the
// script reads it from your own installation and writes it to a gitignored
// folder. Run it once after installing or updating the app.
//
//   node scripts/extract-vendor-webview.mjs
//   node scripts/extract-vendor-webview.mjs --asar "D:\path\to\app.asar"
//
// Only `webview/**` is copied (the renderer: index.html + its chunks). The asar
// container is read directly - no dependency on @electron/asar.
import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);

const repo = path.resolve(path.dirname(new URL(import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, "$1")), "..");
const destination = path.join(repo, "desktop", "vendor-webview");

function findAsar() {
  const explicit = process.argv.indexOf("--asar");
  if (explicit > 0) return process.argv[explicit + 1];
  const roots = [
    "C:/Program Files/WindowsApps",
    "C:/Program Files/OpenAI",
    process.env.LOCALAPPDATA ? path.join(process.env.LOCALAPPDATA, "Programs") : null,
    "/Applications/ChatGPT.app/Contents/Resources",
    os.homedir() + "/Applications/ChatGPT.app/Contents/Resources",
  ].filter(Boolean);
  const candidates = [];
  for (const root of roots) {
    if (!fs.existsSync(root)) continue;
    let entries;
    try {
      entries = fs.readdirSync(root);
    } catch {
      continue; // WindowsApps denies enumeration; the PowerShell fallback below handles it
    }
    for (const entry of entries) {
      for (const candidate of [
        path.join(root, entry, "app", "resources", "app.asar"),
        path.join(root, entry, "resources", "app.asar"),
      ]) {
        if (fs.existsSync(candidate)) candidates.push(candidate);
      }
    }
  }
  if (candidates.length) return candidates.sort((a, b) => fs.statSync(b).mtimeMs - fs.statSync(a).mtimeMs)[0];

  // Windows: ask the package manager where the app lives.
  if (process.platform === "win32") {
    try {
      const { execFileSync } = require("node:child_process");
      const install = execFileSync("powershell", ["-NoProfile", "-Command", "(Get-AppxPackage OpenAI.Codex | Select-Object -First 1).InstallLocation"], { encoding: "utf8" }).trim();
      const candidate = path.join(install, "app", "resources", "app.asar");
      if (install && fs.existsSync(candidate)) return candidate;
    } catch {}
  }
  return undefined;
}

/** Chromium's Pickle header: [len=4][headerSize][jsonSize][json…][files…] */
function readAsarHeader(asarPath) {
  const fd = fs.openSync(asarPath, "r");
  const head = Buffer.alloc(16);
  fs.readSync(fd, head, 0, 16, 0);
  const headerSize = head.readUInt32LE(4);
  const headerBuf = Buffer.alloc(headerSize);
  fs.readSync(fd, headerBuf, 0, headerSize, 8);
  // Chromium Pickle: length-prefixed string with padding; locate the JSON itself
  // rather than trusting the prefix arithmetic.
  const text = headerBuf.toString("utf8");
  const start = text.indexOf("{");
  const json = text.slice(start, text.lastIndexOf("}") + 1);
  fs.closeSync(fd);
  return { header: JSON.parse(json), dataOffset: 8 + headerSize };
}

function collect(node, prefix, out) {
  for (const [name, entry] of Object.entries(node.files ?? {})) {
    const full = prefix ? `${prefix}/${name}` : name;
    if (entry.files) collect(entry, full, out);
    else out.push({ path: full, offset: Number(entry.offset), size: entry.size });
  }
  return out;
}

const asarPath = findAsar();
if (!asarPath) {
  console.error("no ChatGPT app.asar found - pass one with --asar <path>");
  process.exit(1);
}
console.log("asar:", asarPath);

const { header, dataOffset } = readAsarHeader(asarPath);
const wanted = collect(header, "", []).filter((entry) => entry.path.startsWith("webview/"));
if (!wanted.length) {
  console.error("no webview/ folder in that archive");
  process.exit(1);
}

const fd = fs.openSync(asarPath, "r");
let bytes = 0;
for (const entry of wanted) {
  const buffer = Buffer.alloc(entry.size);
  fs.readSync(fd, buffer, 0, entry.size, dataOffset + entry.offset);
  const out = path.join(destination, entry.path);
  fs.mkdirSync(path.dirname(out), { recursive: true });
  fs.writeFileSync(out, buffer);
  bytes += entry.size;
}
fs.closeSync(fd);
console.log(`extracted ${wanted.length} files (${(bytes / 1048576).toFixed(1)} MB) -> ${destination}`);
