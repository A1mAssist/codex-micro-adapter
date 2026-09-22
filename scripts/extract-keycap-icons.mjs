import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

// Regenerates desktop/ui/icons.js from the ChatGPT app's own webview bundle.
// Run scripts/extract-vendor-webview.mjs first so the bundle exists.
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const assets = (process.env.CODEX_MICRO_VENDOR_ASSETS ?? path.join(root, "desktop/vendor-webview/webview/assets")) + path.sep;
const outFile = path.join(root, "desktop/ui/icons.js");
const read = (name) => fs.readFileSync(assets + name, "utf8");
/** Largest bundle whose filename starts with `prefix` (hashes change with every app build). */
const pick = (prefix) => {
  const hits = fs.readdirSync(assets).filter((f) => f.startsWith(prefix) && f.endsWith(".js"));
  if (!hits.length) throw new Error(`no bundle matches ${prefix} in ${assets}`);
  return hits.sort((a, b) => fs.statSync(assets + b).size - fs.statSync(assets + a).size)[0];
};

const files = {
  keyboard: pick("codex-micro-keyboard-surface-"),
  "app-initial": pick("app-initial-"),
  "app-primary": pick("app-primary-"),
  "app-shared": pick("app-shared-"),
  "play-outline": pick("play-outline-"),
};
const sources = Object.fromEntries(Object.entries(files).map(([key, file]) => [key, read(file)]));
const ks = sources.keyboard;

/* ---- brace/string aware scanning ---------------------------------------- */
function readString(text, start) {
  const quote = text[start];
  let i = start + 1;
  while (i < text.length && text[i] !== quote) i += text[i] === "\\" ? 2 : 1;
  return { value: text.slice(start + 1, i), end: i + 1 };
}
function objectText(text, start) {
  let depth = 0;
  for (let i = start; i < text.length; i += 1) {
    const ch = text[i];
    if (ch === "`" || ch === '"' || ch === "'") { i = readString(text, i).end - 1; continue; }
    if (ch === "{") depth += 1;
    else if (ch === "}") { depth -= 1; if (depth === 0) return text.slice(start + 1, i); }
  }
  return "";
}
/** Same walk as objectText, but keeps where the object ends. */
function objectRange(text, start) {
  let depth = 0;
  for (let i = start; i < text.length; i += 1) {
    const ch = text[i];
    if (ch === "`" || ch === '"' || ch === "'") { i = readString(text, i).end - 1; continue; }
    if (ch === "{") depth += 1;
    else if (ch === "}") { depth -= 1; if (depth === 0) return { text: text.slice(start + 1, i), end: i }; }
  }
  return { text: "", end: -1 };
}
/** Index of the bracket that closes text[start] ('[' or '('). */
function matchBracket(text, start) {
  let depth = 0;
  for (let i = start; i < text.length; i += 1) {
    const ch = text[i];
    if (ch === "`" || ch === '"' || ch === "'") { i = readString(text, i).end - 1; continue; }
    if (ch === "[" || ch === "{" || ch === "(") depth += 1;
    else if (ch === "]" || ch === "}" || ch === ")") { depth -= 1; if (depth === 0) return i; }
  }
  return -1;
}
/** The `children` value of the jsx object starting at `brace`, as its own slice. */
function childrenOf(text, brace) {
  const obj = objectRange(text, brace);
  const rel = obj.text.search(/\bchildren\s*:/);
  if (rel < 0) return null;
  let i = brace + 1 + rel + obj.text.slice(rel).indexOf(":") + 1;
  while (i < text.length && /\s/.test(text[i])) i += 1;
  if (text[i] === "[") {
    const close = matchBracket(text, i);
    return close < 0 ? null : { text: text.slice(i + 1, close), end: close };
  }
  return { text: text.slice(i, obj.end), end: obj.end };
}
function splitTop(text) {
  const parts = [];
  let depth = 0;
  let current = "";
  for (let i = 0; i < text.length; i += 1) {
    const ch = text[i];
    if (ch === "`" || ch === '"' || ch === "'") { const s = readString(text, i); current += text.slice(i, s.end); i = s.end - 1; continue; }
    if (ch === "{" || ch === "[" || ch === "(") depth += 1;
    if (ch === "}" || ch === "]" || ch === ")") depth -= 1;
    if (ch === "," && depth === 0) { parts.push(current); current = ""; continue; }
    current += ch;
  }
  parts.push(current);
  return parts;
}
const KEBAB = { fillRule: "fill-rule", clipRule: "clip-rule", strokeWidth: "stroke-width", strokeLinecap: "stroke-linecap", strokeLinejoin: "stroke-linejoin", fillOpacity: "fill-opacity", strokeOpacity: "stroke-opacity", strokeDasharray: "stroke-dasharray" };
const TAGS = ["path", "circle", "rect", "line", "polyline", "polygon", "ellipse"];

function attrsOf(body) {
  const attrs = {};
  for (const part of splitTop(body)) {
    const m = part.trim().match(/^([\w$]+)\s*:\s*(.+)$/s);
    if (!m || m[1] === "key" || m[1] === "children") continue;
    const raw = m[2].trim();
    const quote = raw[0];
    const quoted = (quote === "`" || quote === '"' || quote === "'") && raw.length > 1 && raw.endsWith(quote);
    // a quoted value is a literal (even `scale(0.83)`); anything else must not be an expression
    if (!quoted && (raw.includes("(") || raw.startsWith("[") || raw.startsWith("{"))) continue;
    attrs[KEBAB[m[1]] ?? m[1]] = quoted ? raw.slice(1, -1) : raw;
  }
  return attrs;
}
const markup = (tag, attrs) => `<${tag} ${Object.entries(attrs).map(([k, v]) => `${k}="${v}"`).join(" ")}/>`;

/** The element list of ONE icon: stop at the end of its module / next icon. */
function elementsOf(slice) {
  const out = [];
  const attrsText = (attrs) => Object.entries(attrs).map(([k, v]) => `${k}="${v}"`).join(" ");
  // icons may render with jsx()/jsxs() calls or as a Lucide element array
  const sites = [...slice.matchAll(/\(0,[\w$]+\.jsxs?\)\(`([a-z]+)`,|\[`([a-z]+)`,/g)];
  let claimedTo = -1;
  for (const site of sites) {
    if (site.index < claimedTo) continue; // already emitted inside a <g>
    const tag = site[1] ?? site[2];
    if (tag !== "g" && !TAGS.includes(tag)) continue;
    const brace = slice.indexOf("{", site.index + site[0].length);
    if (brace < 0 || brace - site.index > 40) continue;
    const attrs = attrsOf(objectRange(slice, brace).text);
    if (tag === "g") {
      const kids = childrenOf(slice, brace);
      const inner = kids ? elementsOf(kids.text) : [];
      if (!inner.length) continue;
      const a = attrsText(attrs);
      out.push(`<g${a ? ` ${a}` : ""}>${inner.join("")}</g>`);
      claimedTo = kids.end;
      continue;
    }
    if (Object.keys(attrs).length) out.push(markup(tag, attrs));
  }
  return out;
}

function windowFor(source, hitIndex) {
  // one icon only: stop at the next icon definition or at the module end
  const rest = source.slice(hitIndex + 3);
  const ends = [];
  const moduleEnd = rest.indexOf("})))()}");
  if (moduleEnd >= 0) ends.push(moduleEnd + 3);
  const nextDef = rest.search(/[A-Za-z_$][\w$]*\s*=\s*[A-Za-z_$][\w$]*\s*=>\s*\(0,[\w$]+\.jsxs?\)\(\`svg\`/);
  if (nextDef >= 0) ends.push(nextDef + 3);
  const length = ends.length ? Math.min(...ends) : 2000;
  return source.slice(hitIndex, hitIndex + length);
}

/* ---- registry + module resolution --------------------------------------- */
const registry = {};
const regStart = ks.indexOf("Ze={");
for (const m of ks.slice(regStart + 3, ks.indexOf("}})))()}", regStart)).matchAll(/"([^"]+)":\s*([\w$]+)|([\w$]+):\s*([\w$]+)/g)) {
  registry[m[1] ?? m[3]] = m[2] ?? m[4];
}
const aliasImport = {};
for (const m of ks.matchAll(/import\{([^}]+)\}from"\.\/([^"]+)"/g)) {
  const key = Object.keys(files).find((k) => m[2].startsWith(files[k].replace(/\.js$/, "")));
  if (!key) continue;
  for (const part of m[1].split(",")) {
    const pair = part.match(/^([\w$]+)\s+as\s+([\w$]+)$/);
    if (pair) aliasImport[pair[2]] = { module: key, exported: pair[1] };
    else aliasImport[part] = { module: key, exported: part };
  }
}
const exportMaps = {};
for (const [key, source] of Object.entries(sources)) {
  const map = {};
  for (const m of source.matchAll(/export\{([^}]+)\}/g)) {
    for (const part of m[1].split(",")) {
      const pair = part.trim().match(/^([\w$]+)\s+as\s+([\w$]+)$/);
      if (pair) map[pair[2]] ??= pair[1];
    }
  }
  exportMaps[key] = map;
}

/** The presentation attributes the icon's own <svg> carries — children inherit them. */
const PRESENTATION = ["fill", "stroke", "stroke-width", "stroke-linecap", "stroke-linejoin"];
function svgDefaults(slice) {
  const site = slice.match(/jsxs?\)\(`svg`,/);
  if (!site) return null;
  const brace = slice.indexOf("{", site.index + site[0].length);
  if (brace < 0) return null;
  const attrs = attrsOf(objectRange(slice, brace).text);
  const out = {};
  for (const key of PRESENTATION) if (attrs[key] != null) out[key] = attrs[key];
  return Object.keys(out).length ? out : null;
}

function extract(source, local) {
  const re = new RegExp(`\\b${local}\\s*=\\s*[\\w$]+\\s*=>`, "g");
  for (const hit of source.matchAll(re)) {
    const slice = windowFor(source, hit.index);
    const viewBox = slice.match(/viewBox:\s*`([^`]+)`/)?.[1] ?? "0 0 24 24";
    const elements = elementsOf(slice);
    if (elements.length) return { box: viewBox, defaults: svgDefaults(slice), body: elements.join("") };
  }
  return null;
}

const icons = {};
const missing = [];
const sizes = {};
for (const [id, alias] of Object.entries(registry)) {
  const origin = aliasImport[alias];
  const candidates = [];
  if (origin) {
    const local = exportMaps[origin.module]?.[origin.exported];
    if (local) candidates.push([origin.module, local]);
    for (const [key, map] of Object.entries(exportMaps)) if (map[origin.exported]) candidates.push([key, map[origin.exported]]);
  }
  candidates.push(["keyboard", alias]);
  let found = null;
  for (const [key, local] of candidates) {
    if (!local || !sources[key]) continue;
    found = extract(sources[key], local);
    if (found) break;
  }
  if (found) { icons[id] = found; sizes[id] = found.body.length; }
  else missing.push(`${id}(${alias})`);
}

// the "empty" caps: no registry entry, the preview shows Lucide's square-pen
const penFile = pick("square-pen-");
const penSource = read(penFile);
icons.empty = {
  box: penSource.match(/viewBox:\s*`([^`]+)`/)?.[1] ?? "0 0 24 24",
  body: elementsOf(penSource.slice(0, penSource.indexOf("export{"))).join(""),
  defaults: { fill: "none", stroke: "currentColor", "stroke-width": "2", "stroke-linecap": "round", "stroke-linejoin": "round" },
};

console.log("app icons:", Object.keys(icons).length - 1, "/", Object.keys(registry).length, "(+1 Lucide fallback)");
console.log("missing:", missing.join(", ") || "none");
console.log("sizes:", Object.entries(sizes).sort((a, b) => b[1] - a[1]).slice(0, 6).map(([id, n]) => `${id}=${n}`).join(" "));

const lines = [
  "// Keycap icons, extracted verbatim from the ChatGPT app's own icon components",
  "// (codex-micro-keyboard-surface / app-initial / app-primary / app-shared).",
  "//",
  "// One entry per catalogue id: the app's own viewBox, its own elements, and the",
  "// presentation attributes its root <svg> declares — children inherit them.",
  "",
  "export const ICONS = {",
];
for (const [id, icon] of Object.entries(icons)) {
  const defaults = icon.defaults ? ` defaults: ${JSON.stringify(icon.defaults)},` : "";
  lines.push(`  ${JSON.stringify(id)}: { box: ${JSON.stringify(icon.box)},${defaults} body: ${JSON.stringify(icon.body)} },`);
}
lines.push(
  "};",
  "",
  "/** Inline SVG for one keycap icon, or '' when the app has no icon for it. */",
  "export function iconSvg(id, className = 'keycap-icon') {",
  "  const icon = ICONS[id];",
  "  if (!icon) return '';",
  "  const attrs = Object.entries(icon.defaults ?? {}).map(([k, v]) => `${k}=\"${v}\"`).join('');",
  "  return `<svg class=\"${className}\" viewBox=\"${icon.box}\" xmlns=\"http://www.w3.org/2000/svg\"${attrs}>${icon.body}</svg>`;",
  "}",
  "",
);
fs.writeFileSync(outFile, lines.join("\n"));
console.log("wrote", outFile);
