# Harnesses

Five harnesses are supported, each in two directions:

- **Harness to keyboard.** A hook or plugin reports `session <id> <status>` on
  the host's loopback socket, and the host gives that session one of the six
  agent keys. See the control-socket section of the main README.
- **Keyboard to harness.** Keys and stick resolve through `bindings` in
  `%APPDATA%\codex-micro\config.json` to real keystrokes, which land in whatever
  window has focus. Ready-made maps live in [`presets/`](../presets/README.md).

| Harness | Ships as | Events used | Verified |
| --- | --- | --- | --- |
| Claude Code | `plugins/codex-micro` (plugin) | 8 hooks | runtime: a real session lit the keys |
| Codex CLI | `plugins/codex-micro` (same plugin, Codex manifest) | 7 hooks | runtime: a real session lit the keys |
| pi | `plugins/pi/codex-micro.ts` | extension events | `scripts/check-harness-adapters.mjs` |
| opencode | `plugins/opencode/codex-micro.ts` | plugin event bus | `scripts/check-harness-adapters.mjs` |
| DeepSeek Harness (`dsh`) | `plugins/deepseek/` | hooks bridge | checked against the official source |

## Claude Code

```bash
claude plugin marketplace add A1mAssist/codex-micro-adapter
claude plugin install codex-micro@codex-micro-adapter
```

Session start, prompt submit, tool use, Stop, Notification, compaction and
session end all forward `session_id`; nothing else to configure.

| Hook | Agent key |
| --- | --- |
| `SessionStart` | Idle |
| `UserPromptSubmit`, `PreToolUse`, `SubagentStop`, `PreCompact` | Working |
| `Notification` (waiting for you) | Awaiting approval |
| `Stop` | Unread |
| `SessionEnd` | Off, key released |

Preset: [`presets/claude-code.json`](../presets/claude-code.json) - `enter`
confirms, `escape` declines, `shift+tab` cycles the permission mode.

## Codex CLI

```powershell
codex plugin marketplace add A1mAssist/codex-micro-adapter
codex plugin add codex-micro@codex-micro-adapter
```

The first interactive `codex` run shows **Hooks need review** - choose
*Trust all and continue*. Measured with the same plugin and the same host: a
trusted run delivered `idle`/`working`/`end`, while an untrusted `codex exec`
delivered nothing at all, not even the hook script's own debug log. That is
Codex's own sandbox policy, not a bug here. `codex exec` can bypass the prompt
for one run with `--dangerously-bypass-hook-trust`.

| Hook | Agent key |
| --- | --- |
| `SessionStart` | Idle |
| `UserPromptSubmit`, `SubagentStop` | Working |
| `PermissionRequest` (Codex has no `Notification`) | Awaiting approval |
| `Interrupt` | Idle |
| `Stop` | Unread |
| `SessionEnd` | Off, key released |

No keyboard preset yet: the Codex CLI keymap has not been verified against its
own binary, and guessing it would be worse than leaving it unbound.

## Pi

Pi (`earendil-works/pi`, MIT) has no hooks config; its extension API is the
surface. Copy [`plugins/pi/codex-micro.ts`](../plugins/pi/codex-micro.ts) to
`~/.pi/agent/extensions/` (every project) or `<repo>/.pi/extensions/` (one
project, which needs project trust). The session id comes from
`ctx.sessionManager.getSessionId()`.

| Extension event | Agent key |
| --- | --- |
| `session_start` | Idle |
| `input`, `tool_call`, `tool_execution_start`, `ui_prompt_end` | Working |
| `ui_prompt_start` | Awaiting approval |
| `agent_settled` | Unread |
| `session_shutdown` | Off, key released |

`agent_end` is deliberately not wired: it fires between turns, while
`agent_settled` is the "pi has really stopped" signal.

Preset: [`presets/pi.json`](../presets/pi.json) - `enter` confirms a
select/confirm dialog, `escape` cancels it.

## opencode

Copy [`plugins/opencode/codex-micro.ts`](../plugins/opencode/codex-micro.ts) to
`.opencode/plugins/` (one project) or `~/.config/opencode/plugins/` (every
project). opencode runs plugins in-process, so this one talks to the control
port directly; unmapped events send nothing.

| opencode event | Agent key |
| --- | --- |
| `session.created` | Idle |
| `session.idle` | Unread |
| `permission.asked` | Awaiting approval |
| `session.deleted` | Off, key released |

Preset: [`presets/opencode.json`](../presets/opencode.json) - only submit is
bound, because opencode's approval dialog has no documented key tokens.

## DeepSeek Harness

The official `dsh` (MIT, developer preview) has no TUI: it ships a Web UI, an
ACP stdio server and a Codex-style hooks bridge. This adapter uses the hooks
bridge - see [`plugins/deepseek/README.md`](../plugins/deepseek/README.md) for
the plugin install, the `cordis.patch.yml` entry and the `<REPO>` placeholder
that has to be filled in.

| `dsh` hook | Agent key |
| --- | --- |
| `SessionStart` | Idle |
| `UserPromptSubmit` | Working |
| `Stop` | Unread |

Two states are missing from that bridge: `PermissionRequest` is dropped, and
there is no `SessionEnd`, so a key keeps showing "unread" until another session
takes it. Both exist on `dsh`'s ACP surface (`session/request_permission`,
`session/close`) if they are ever needed.

There is nothing to bind on the keyboard side: approval and stop are plain
buttons in the Web UI with no key tokens.

## Check an adapter

```powershell
node scripts/check-harness-adapters.mjs   # pi + opencode, feeds them real events
cargo test -p codex-micro-backend         # the host itself
```

## Not shipped

Qwen Code, Gemini CLI, Goose and Continue CLI use the same hook shape as Claude
Code, so an adapter for any of them is one config file - but they are not
maintained here. Crush exposes only `PreToolUse` (no session start, no turn
end), Aider only a `--notifications-command` with no payload, Cline's hooks do
not run on Windows, and Roo Code has no agent event surface at all.
