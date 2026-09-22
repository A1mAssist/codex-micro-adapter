# Other harnesses

The host is harness-agnostic in both directions:

- **Harness to keyboard.** A hook in the harness reports `session <id> <status>`
  on the loopback socket, and the host hands that session one of the six agent
  keys (see the control-socket section of the main README). Any language works;
  the bundled `plugins/codex-micro/hooks/report.mjs` is a small Node script that
  maps a hook payload to a status.
- **Keyboard to harness.** Keys and stick resolve through `bindings` in
  `%APPDATA%\codex-micro\config.json` to real keystrokes, which land in whatever
  window has focus. That needs no per-harness code at all, only a preset.

Everything below was checked against each harness's own docs or source on
2026-09-22; the two that are installed here were also run end to end.

| Harness | Event surface | Session id | Ships as | State |
| --- | --- | --- | --- | --- |
| Claude Code | 9 hooks incl. SessionStart / Notification / Stop / SessionEnd | `session_id` | plugin in this repo | runtime-tested |
| Codex CLI | 12 hooks incl. SessionStart / PermissionRequest / Stop / Interrupt | `session_id` | plugin in this repo | runtime-tested |
| Qwen Code | 21 hooks incl. Stop / Notification / PermissionRequest | `session_id` | `~/.qwen/settings.json` | docs-checked |
| Gemini CLI | 11 hooks incl. AfterAgent / Notification | `session_id` | `~/.gemini/settings.json` | docs-checked |
| Goose | 11 hooks incl. Stop / SessionEnd | `session_id` | plugin directory | docs-checked |
| opencode | plugin bus: session.created / session.idle / permission.asked | `sessionID` | `.opencode/plugins/*.js` | docs-checked |
| Continue CLI | Claude-Code-compatible hooks | `session_id` | reuses the Claude Code plugin | docs-checked |
| Crush | PreToolUse only | `session_id` | keystrokes only | docs-checked |
| Aider | no hooks; notifications-command only | none | keystrokes only | docs-checked |

## Claude Code

```powershell
claude plugin marketplace add A1mAssist/codex-micro-adapter
claude plugin install codex-micro@codex-micro-adapter
```

Every hook forwards `session_id`; there is nothing else to configure.

## Codex CLI

```powershell
codex plugin marketplace add A1mAssist/codex-micro-adapter
codex plugin add codex-micro@codex-micro-adapter
```

The first interactive `codex` run shows **Hooks need review** - choose
*Trust all and continue*. Until you do, the keys stay dark: measured with the
same plugin and the same host, a trusted run delivered
`idle`/`working`/`end`, while an untrusted `codex exec` delivered nothing at all -
not even the hook script's own debug log. That is Codex's own policy (untrusted
hooks stay in the sandbox), not a bug here. `codex exec` can bypass the prompt
for one run with `--dangerously-bypass-hook-trust`.

## Qwen Code

`~/.qwen/settings.json`, or a project `.qwen/settings.json`. `async` keeps the
hook off the critical path:

```json
{
  "hooks": {
    "SessionStart": [{ "hooks": [{ "type": "command", "async": true, "command": "node \"<repo>/plugins/codex-micro/hooks/report.mjs\"" }] }],
    "UserPromptSubmit": [{ "hooks": [{ "type": "command", "async": true, "command": "node \"<repo>/plugins/codex-micro/hooks/report.mjs\"" }] }],
    "Stop": [{ "hooks": [{ "type": "command", "async": true, "command": "node \"<repo>/plugins/codex-micro/hooks/report.mjs\"" }] }],
    "SessionEnd": [{ "hooks": [{ "type": "command", "async": true, "command": "node \"<repo>/plugins/codex-micro/hooks/report.mjs\"" }] }]
  }
}
```

Keys: interrupt `Ctrl+C`, cancel `Esc`, cycle approval mode `Shift+Tab`
(`Tab` on Windows), confirm `Enter`. A new session is `/new`, not a key.

## Gemini CLI

`~/.gemini/settings.json`. Gemini's `timeout` is in milliseconds, and a hook's
stdout must be JSON only (write diagnostics to stderr):

```json
{
  "hooks": {
    "SessionStart": [{ "matcher": "startup", "hooks": [{ "name": "micro-start", "type": "command", "command": "node \"<repo>/plugins/codex-micro/hooks/report.mjs\"", "timeout": 2000 }] }],
    "AfterAgent": [{ "hooks": [{ "name": "micro-stop", "type": "command", "command": "node \"<repo>/plugins/codex-micro/hooks/report.mjs\"", "timeout": 2000 }] }],
    "SessionEnd": [{ "hooks": [{ "name": "micro-end", "type": "command", "command": "node \"<repo>/plugins/codex-micro/hooks/report.mjs\"", "timeout": 2000 }] }]
  }
}
```

Keys: `Enter` confirm, `Esc` cancel, `Ctrl+C` interrupt.

## Goose

Goose plugins live in `~/.agents/plugins/<name>/` with a `plugin.json` and a
`hooks/hooks.json`. Use the same `SessionStart` / `UserPromptSubmit` / `Stop` /
`SessionEnd` entries as Qwen Code, with the command pointing at `report.mjs`.
Goose matchers are regular expressions - leave `matcher` out rather than writing
`"*"`, which is not a valid one.

Goose documents desktop shortcuts only, so its CLI keymap is **unverified**.

## opencode

`.opencode/plugins/codex-micro.js`, or globally in
`~/.config/opencode/plugins/`. opencode plugins run in-process, so this one
talks to the control port directly:

```js
import net from "node:net";

const STATUS = {
  "session.created": "idle",
  "session.idle": "unread",
  "permission.asked": "awaiting-approval",
  "session.deleted": "end",
};

export const CodexMicro = async () => ({
  event: async ({ event }) => {
    const status = STATUS[event?.type];
    const session = event?.properties?.sessionID;
    if (!status || !session) return;
    const socket = net.connect({
      host: "127.0.0.1",
      port: Number(process.env.CODEX_MICRO_PORT ?? 27700),
    });
    socket.on("error", () => {});
    socket.on("connect", () => socket.end(`session ${session} ${status}\n`));
  },
});
```

Keys: `escape` interrupts, `ctrl+x n` opens a session, `ctrl+x q` quits. The
approval dialog's keys are **unverified**.

## Continue CLI

Continue's CLI reads `~/.continue/settings.json` and also Claude Code's own
`settings.json`, with the same hook names and the same `session_id` payload, so
the Claude Code plugin above already covers it. Nothing extra to ship.

## Crush and Aider

Both can only do half the job, so they get keystrokes only:

- **Crush** fires a single `PreToolUse` hook - no session start, no turn end -
  so it cannot drive an agent key honestly. `Ctrl+P` opens the command palette,
  `Esc` closes dialogs.
- **Aider** has no hooks, only `--notifications-command`, and that command gets
  no arguments and no session id. For one key that says "Aider is waiting", run
  `report.mjs unread` with `CODEX_MICRO_SESSION=aider` in the environment.
  Approvals are typed (`y` / `n` plus `Enter`), so bind those.
