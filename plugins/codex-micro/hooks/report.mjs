// Reports one harness session's state to the Codex Micro host.
//
//   node report.mjs                 # status comes from the payload's hook_event_name
//   node report.mjs working         # or say it yourself
//
// Claude Code, Codex CLI, Gemini CLI, Qwen Code and Goose all hand a hook a JSON
// payload on stdin with `session_id` + `hook_event_name`, so this one script
// serves every one of them: it forwards the session id and lets the host pick
// the agent key, and six parallel terminals need no per-shell setup.
//
// CODEX_MICRO_AGENT pins this session to one key (0-5), CODEX_MICRO_SESSION
// overrides the id the host sees, CODEX_MICRO_PORT moves off 27700.
//
// Never fails a session: if the host is not running (or the socket is slow) the
// hook still exits 0 straight away.
import fs from "node:fs";
import net from "node:net";

/** CODEX_MICRO_DEBUG=<file> leaves a trace of what a hook actually did. */
const trace = (message) => {
  const file = process.env.CODEX_MICRO_DEBUG;
  if (!file) return;
  try {
    fs.appendFileSync(file, `${new Date().toISOString()} ${message}\n`);
  } catch {
    /* a debug log must never break the hook */
  }
};

/** hook_event_name, lower-cased, to the status the agent key should show. */
const STATUS = new Map(
  Object.entries({
    sessionstart: "idle",
    session_start: "idle",
    "session.created": "idle",
    userpromptsubmit: "working",
    user_prompt_submit: "working",
    pretooluse: "working",
    pre_tool_use: "working",
    posttooluse: "working",
    post_tool_use: "working",
    posttoolusefailure: "error",
    subagentstart: "working",
    subagent_start: "working",
    subagentstop: "working",
    subagent_stop: "working",
    precompact: "working",
    postcompact: "working",
    beforeagent: "working",
    afteragent: "unread",
    notification: "awaiting-approval",
    permissionrequest: "awaiting-approval",
    permission_request: "awaiting-approval",
    "permission.asked": "awaiting-approval",
    stop: "unread",
    stopfailure: "error",
    interrupt: "idle",
    "session.idle": "unread",
    sessionend: "end",
    session_end: "end",
    "session.deleted": "end",
  }),
);

/** The hook payload on stdin: session_id, hook_event_name, cwd, … */
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
const event = String(payload?.hook_event_name ?? payload?.event ?? "").trim().toLowerCase();
const status = process.argv[2] || STATUS.get(event);

if (status) {
  const session =
    process.env.CODEX_MICRO_SESSION ||
    payload?.session_id ||
    payload?.sessionID ||
    payload?.sessionId ||
    payload?.properties?.sessionID ||
    "";
  if (session) {
    const port = Number(process.env.CODEX_MICRO_PORT ?? 27700);
    const send = (line) => new Promise((resolve) => {
      const socket = net.connect({ host: "127.0.0.1", port });
      let reply = "";
      let settled = false;
      const finish = () => {
        if (settled) return;
        settled = true;
        socket.destroy();
        resolve(reply.trim());
      };

      trace(`${line} -> 127.0.0.1:${port}`);
      socket.setTimeout(300, () => {
        trace(`timed out talking to 127.0.0.1:${port}`);
        finish();
      });
      socket.on("connect", () => socket.write(`${line}\n`));
      socket.on("data", (chunk) => {
        reply += chunk;
        if (reply.includes("\n")) finish();
      });
      socket.on("end", finish);
      socket.on("error", (error) => {
        trace(`could not reach 127.0.0.1:${port}: ${error.message}`);
        finish();
      });
    });

    const rawAgent = process.env.CODEX_MICRO_AGENT;
    const agent = rawAgent == null || rawAgent === "" ? null : Number(rawAgent);
    if (agent != null && (!Number.isInteger(agent) || agent < 0 || agent > 5)) {
      trace(`invalid CODEX_MICRO_AGENT=${rawAgent}; using automatic assignment`);
    }
    const pinned = agent == null || agent < 0 || agent > 5
      ? "ok"
      : await send(`pin ${session} ${agent}`);
    if (pinned.startsWith("ok")) {
      await send(`session ${session} ${status}`);
    } else {
      trace(pinned || `could not pin session ${session}`);
    }
  } else {
    trace(`nothing to send: event=${event || "(none)"}`);
  }
}
