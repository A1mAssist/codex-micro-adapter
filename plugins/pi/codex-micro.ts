// Codex Micro adapter for the Pi agent harness (pi.dev).
//
// Copy this file to `~/.pi/agent/extensions/codex-micro.ts` (every project) or
// `<repo>/.pi/extensions/codex-micro.ts` (one project, needs project trust),
// start the Codex Micro host, and each pi session gets an agent key of its own -
// the host hands out the keys, so parallel sessions need no numbering.
//
// Nothing leaves the machine: each event opens a short-lived TCP connection to
// the host's loopback port and sends one line. No host running = nothing
// happens, the harness never blocks on it.
import net from "node:net";
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";

/** pi lifecycle event -> the status our agent key should show. */
const STATUS: Record<string, string> = {
  session_start: "idle",
  input: "working",
  tool_call: "working",
  tool_execution_start: "working",
  ui_prompt_start: "awaiting-approval",
  ui_prompt_end: "working",
  // agent_end fires between turns; agent_settled means pi has really stopped
  agent_settled: "unread",
  session_shutdown: "end",
};

function report(session: string, status: string) {
  const socket = net.connect({
    host: "127.0.0.1",
    port: Number(process.env.CODEX_MICRO_PORT ?? 27700),
  });
  socket.on("error", () => {});
  socket.on("connect", () => socket.end(`session ${session} ${status}\n`));
}

interface SessionContext {
  sessionManager?: { getSessionId?: () => string };
}

export default function (pi: ExtensionAPI) {
  // `pi.on` is typed against its own event union, which grows between releases;
  // one loose signature keeps this extension loading across versions.
  const on = pi.on as unknown as (
    event: string,
    handler: (event: unknown, ctx: SessionContext) => void,
  ) => void;

  for (const [event, status] of Object.entries(STATUS)) {
    on(event, (_event, ctx) => {
      const session = ctx?.sessionManager?.getSessionId?.();
      if (session) report(session, status);
    });
  }
}
