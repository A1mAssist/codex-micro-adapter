// Runnable check for the harness adapters that are plain JavaScript/TypeScript:
//
//   node scripts/check-harness-adapters.mjs
//
// It loads the pi extension and the opencode plugin, feeds them the events their
// harness would send, and asserts the lines that reach a stand-in host on
// 127.0.0.1. Nothing here needs pi or opencode to be installed; importing the
// two .ts files directly needs Node 23.6+ (or 22.6 with
// --experimental-strip-types).
import net from "node:net";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const received = [];

const server = net.createServer((socket) => {
  let buffer = "";
  socket.on("data", (chunk) => (buffer += chunk));
  socket.on("end", () => {
    const line = buffer.trim();
    if (line) received.push(line);
  });
});
await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
process.env.CODEX_MICRO_PORT = String(server.address().port);

let failures = 0;

/** Wait for `expected` lines (or a timeout), then compare them all at once. */
async function expect(label, expected) {
  const deadline = Date.now() + 1500;
  while (received.length < expected.length && Date.now() < deadline) {
    await new Promise((resolve) => setTimeout(resolve, 25));
  }
  const got = received.splice(0);
  const ok = got.length === expected.length && got.every((line, i) => line === expected[i]);
  if (!ok) failures += 1;
  console.log(`${ok ? "ok  " : "FAIL"} ${label}`);
  if (!ok) console.log(`     expected ${JSON.stringify(expected)}\n     got      ${JSON.stringify(got)}`);
}

// --- pi extension ----------------------------------------------------------
const pi = (await import(pathToFileURL(path.join(root, "plugins/pi/codex-micro.ts")).href)).default;
const handlers = new Map();
pi({ on: (event, handler) => handlers.set(event, handler) });
const piContext = { sessionManager: { getSessionId: () => "pi-1" } };

for (const event of ["session_start", "input", "ui_prompt_start", "agent_settled", "session_shutdown"]) {
  const handler = handlers.get(event);
  if (!handler) {
    console.log(`FAIL pi registered no handler for ${event}`);
    failures += 1;
    continue;
  }
  handler(undefined, piContext);
}

await expect("pi lifecycle events", [
  "session pi-1 idle",
  "session pi-1 working",
  "session pi-1 awaiting-approval",
  "session pi-1 unread",
  "session pi-1 end",
]);

// --- opencode plugin -------------------------------------------------------
const opencode = (await import(pathToFileURL(path.join(root, "plugins/opencode/codex-micro.ts")).href)).CodexMicro;
const plugin = await opencode();

for (const type of ["session.created", "session.idle", "permission.asked", "session.deleted"]) {
  await plugin.event({ event: { type, properties: { sessionID: "oc-1" } } });
}

await expect("opencode session events", [
  "session oc-1 idle",
  "session oc-1 unread",
  "session oc-1 awaiting-approval",
  "session oc-1 end",
]);

// an event we do not map must send nothing at all
await plugin.event({ event: { type: "message.updated", properties: { sessionID: "oc-1" } } });
await expect("opencode ignores unmapped events", []);

// --- dsh native Cordis plugin ----------------------------------------------
const dsh = await import(pathToFileURL(path.join(root, "plugins/deepseek/plugin/index.js")).href);
const seams = new Map();
dsh.apply({ on: (event, handler) => seams.set(event, handler) }, {});

/** Fire one seam and report whether the plugin handed the request on. */
function seam(name, payload) {
  const handler = seams.get(name);
  if (!handler) {
    console.log(`FAIL dsh registered no handler for ${name}`);
    failures += 1;
    return false;
  }
  let passed = false;
  handler(payload, () => {
    passed = true;
  });
  return passed;
}

const dshAgent = { session: { header: { id: "dsh-1" } } };
const seamsCalled = [
  seam("agent/created", { agent: dshAgent }),
  seam("agent/pre-step", { agent: dshAgent, messages: [{}] }),
  seam("tools/pre-execute", { agent: dshAgent }),
  seam("approval/request", { agent: dshAgent }),
  seam("agent/turn-stopping", { agent: dshAgent }),
  seam("session/disposed", dshAgent.session),
];

await expect("dsh lifecycle seams", [
  "session dsh-1 idle",
  "session dsh-1 working",
  "session dsh-1 working",
  "session dsh-1 awaiting-approval",
  "session dsh-1 unread",
  "session dsh-1 end",
]);

// Waterfall seams must call next(): swallowing one stalls the harness.
if (!seamsCalled.slice(1, 4).every(Boolean)) {
  console.log("FAIL dsh dropped a waterfall request instead of calling next()");
  failures += 1;
}

// Delegated work must not take one of the six keys.
for (const header of [{ id: "dsh-sub", origin: "subagent" }, { id: "dsh-deep", delegationDepth: 1 }]) {
  seam("agent/created", { agent: { session: { header } } });
  seam("agent/pre-step", { agent: { session: { header } }, messages: [{}] });
  seam("session/disposed", { header });
}
seam("agent/created", { agent: { session: { header: { id: "dsh-main" } } } });

await expect("dsh skips subagents", ["session dsh-main idle"]);

// --- dsh browser half ------------------------------------------------------
// The page half polls the keyboard host; here the poll and the clock are fakes.
let tick;
const realSetInterval = globalThis.setInterval;
const realFetch = globalThis.fetch;
globalThis.setInterval = (fn) => {
  tick = fn;
  return 0;
};
let answer = { seq: 1, session: null };
globalThis.fetch = async () => ({ json: async () => answer });

const dshClient = await import(pathToFileURL(path.join(root, "plugins/deepseek/plugin/client.js")).href);
const opened = [];
dshClient.apply({
  uiWorkspace: { openSession: (id) => opened.push(id) },
  effect: (fn) => fn(),
});
await tick(); // the first answer only says where the sequence stands
answer = { seq: 2, session: "dsh-9" };
await tick(); // a new tap: the page should follow it
const afterTap = opened.at(-1);
answer = { seq: 3, session: null };
await tick(); // a tap that belongs to no session opens nothing

globalThis.setInterval = realSetInterval;
globalThis.fetch = realFetch;

if (afterTap !== "dsh-9") {
  console.log("FAIL dsh page did not follow the tap: " + JSON.stringify(opened));
  failures += 1;
} else if (opened.at(-1) !== "dsh-9") {
  console.log("FAIL dsh page opened something for a sessionless tap: " + JSON.stringify(opened));
  failures += 1;
} else {
  console.log("ok   dsh browser half follows an agent-key tap");
}

server.close();
if (failures) {
  console.error(`\n${failures} harness adapter check(s) failed`);
  process.exit(1);
}
console.log("\nharness adapters: all checks passed");
