// Reports this Claude Code session's state to the Codex Micro host.
//
//   node report.mjs working
//
// Claude Code hands every hook a JSON payload on stdin, and that payload carries
// the session id. We forward the id and let the host pick the agent key, so six
// terminals need no per-shell numbering. CODEX_MICRO_AGENT still pins one key
// (0-5) when you want that, and CODEX_MICRO_PORT moves off 27700.
//
// Never fails a session: if the host is not running (or the socket is slow) the
// hook still exits 0 straight away.
import net from "node:net";

const state = process.argv[2] ?? "idle";
const port = Number(process.env.CODEX_MICRO_PORT ?? 27700);
const pinned = process.env.CODEX_MICRO_AGENT;

/** The hook payload on stdin: session_id, cwd, hook_event_name, … */
async function hookPayload() {
  if (process.stdin.isTTY) return null;
  let raw = "";
  for await (const chunk of process.stdin) raw += chunk;
  try {
    return JSON.parse(raw);
  } catch {
    return null;
  }
}

const payload = await hookPayload();
const session = process.env.CODEX_MICRO_SESSION ?? payload?.session_id ?? "";

const line = pinned
  ? `agent ${Number(pinned)} ${state === "end" ? "off" : state}\n`
  : session
    ? `session ${session} ${state}\n`
    : null;

if (line) {
  const socket = net.connect({ host: "127.0.0.1", port });
  const done = () => socket.destroy();

  socket.setTimeout(300);
  socket.on("connect", () => socket.end(line, done));
  socket.on("timeout", done);
  socket.on("error", done);
}