// Codex Micro adapter for opencode.
//
// Copy this file to `.opencode/plugins/codex-micro.ts` (one project) or
// `~/.config/opencode/plugins/codex-micro.ts` (every project), start the Codex
// Micro host (`codex-micro-desktop`, or `codex-micro-backend run --live`), and
// each opencode session gets an agent key of its own — the host hands out the
// keys, so parallel sessions need no numbering.
//
// opencode plugins run on Bun, so `node:net` is available. Nothing is sent
// anywhere except the host's loopback port; a missing host is ignored.
import net from "node:net";

/** opencode event -> agent-key status. Extend as opencode grows events. */
const STATUS: Record<string, string> = {
  "session.created": "idle",
  "session.idle": "unread",
  "permission.asked": "awaiting-approval",
  "session.deleted": "end",
};

function send(line: string): Promise<string> {
  return new Promise((resolve) => {
    const socket = net.connect({
      host: "127.0.0.1",
      port: Number(process.env.CODEX_MICRO_PORT ?? 27700),
    });
    let reply = "";
    const finish = () => {
      socket.destroy();
      resolve(reply.trim());
    };
    socket.setTimeout(300, finish);
    socket.on("error", finish);
    socket.on("connect", () => socket.write(`${line}\n`));
    socket.on("data", (chunk) => {
      reply += chunk;
      if (reply.includes("\n")) finish();
    });
    socket.on("end", finish);
  });
}

async function report(session: string, status: string) {
  const rawAgent = process.env.CODEX_MICRO_AGENT;
  const agent = rawAgent == null || rawAgent === "" ? null : Number(rawAgent);
  if (agent != null && Number.isInteger(agent) && agent >= 0 && agent <= 5) {
    const reply = await send(`pin ${session} ${agent}`);
    if (!reply.startsWith("ok")) return;
  }
  await send(`session ${session} ${status}`);
}

export const CodexMicro = async () => ({
  event: async ({
    event,
  }: {
    event: { type: string; properties?: { sessionID?: string } };
  }) => {
    const status = STATUS[event?.type];
    const session = event?.properties?.sessionID;
    if (status && session) await report(session, status);
  },
});
