// Reports one agent-key state to the Codex Micro host.
//
//   node report.mjs working
//
// Never fails a session: if the host is not running (or the socket is slow) the
// hook still exits 0 straight away. The slot defaults to agent key 1; set
// CODEX_MICRO_AGENT (0-5) to spread several sessions across the six keys, and
// CODEX_MICRO_PORT if the host listens somewhere other than 27700.
import net from "node:net";

const state = process.argv[2] ?? "idle";
const agent = Number(process.env.CODEX_MICRO_AGENT ?? 0);
const port = Number(process.env.CODEX_MICRO_PORT ?? 27700);

const socket = net.connect({ host: "127.0.0.1", port });
const done = () => socket.destroy();

socket.setTimeout(300);
socket.on("connect", () => socket.end(`agent ${agent} ${state}\n`, done));
socket.on("timeout", done);
socket.on("error", done);
