/**
 * Codex Micro adapter for the DeepSeek Harness, as a native Cordis plugin.
 *
 * The official `@deepseek-ai/dsh-hooks-codex` bridge covers five Codex hook
 * points, which leaves two agent-key states dark: "waiting for approval" (the
 * bridge drops PermissionRequest) and "session over" (there is no SessionEnd).
 * This plugin listens on the harness seams directly, so it can do both.
 *
 * Every state is one line on the Codex Micro host's loopback socket; nothing is
 * written anywhere else. A host that is not running is ignored.
 *
 * States reported:
 *   agent/created       -> idle
 *   agent/pre-step      -> working
 *   tools/pre-execute   -> working
 *   approval/request    -> awaiting-approval   (observed, never answered)
 *   agent/turn-stopping -> unread
 *   session/disposed    -> end
 */
import net from "node:net";

export const name = "codex-micro";

/** No services are required: the seams below are plain events. */
export const inject = [];

/** `agent.session.header.id` — or `session.header.id` for seams that hand over the session. */
function sessionOf(thing) {
  return thing?.session?.header?.id ?? thing?.header?.id ?? "";
}

/** Some seams hand over the payload, others the thing itself. */
function agentOf(payload) {
  return payload?.agent ?? payload;
}

export function apply(ctx, config) {
  const host = config?.host ?? "127.0.0.1";
  // the env var wins so a test (or a second host) can point somewhere else
  const port = Number(process.env.CODEX_MICRO_PORT ?? config?.port ?? 27700);

  const report = (session, status) => {
    if (!session) return;
    const socket = net.connect({ host, port });
    socket.on("error", () => {});
    socket.on("connect", () => socket.end(`session ${session} ${status}\n`));
  };

  /** Waterfall seams have to hand the request on, or the harness stalls. */
  const pass = (next) => (typeof next === "function" ? next() : undefined);

  ctx.on("agent/created", (payload) => {
    report(sessionOf(agentOf(payload)), "idle");
  });

  ctx.on("agent/pre-step", ({ agent, messages }, next) => {
    if (messages?.length) report(sessionOf(agent), "working");
    return pass(next);
  });

  ctx.on("tools/pre-execute", (exec, next) => {
    report(sessionOf(agentOf(exec)), "working");
    return pass(next);
  });

  // We only watch approvals: `next()` keeps the real answerer in charge, so a
  // missing answerer behaves exactly as it did before this plugin was mounted.
  ctx.on("approval/request", (request, next) => {
    report(sessionOf(agentOf(request)), "awaiting-approval");
    return pass(next);
  });

  ctx.on("agent/turn-stopping", (payload) => {
    report(sessionOf(agentOf(payload)), "unread");
  });

  // The session owns the agent key, so this is the only "the key is free now"
  // signal: compaction rebuilds the agent under the same session and must not
  // drop a key that is still in use.
  ctx.on("session/disposed", (session) => {
    report(sessionOf(session), "end");
  });

  ctx.logger?.info?.(`codex-micro: reporting agent keys to ${host}:${port}`);
}
