# Codex Micro plugin

One plugin, two harnesses: it turns the six agent keys of a Work Louder Codex
Micro into a live view of your sessions. The hooks report state over the host's
loopback socket - nothing is sent anywhere else.

| Harness | Manifest | Hook file |
| --- | --- | --- |
| Claude Code | `.claude-plugin/plugin.json` | `hooks/hooks.json` |
| Codex CLI | `.codex-plugin/plugin.json` | `hooks/codex-hooks.json` |

| Harness event | Agent key shows |
| --- | --- |
| `SessionStart` | Idle (white) |
| `UserPromptSubmit`, `PreToolUse`, `SubagentStop`, `PreCompact` | Working (blue, snaking) |
| `Notification` (Claude), `PermissionRequest` (Codex) | Awaiting approval (orange) |
| `Stop`, `AfterAgent`, `session.idle` | Unread (green) |
| `Interrupt` | Idle |
| `SessionEnd` | Off |

## Install

Claude Code:

```bash
claude plugin marketplace add A1mAssist/codex-micro-adapter
claude plugin install codex-micro@codex-micro-adapter
```

Codex CLI:

```powershell
codex plugin marketplace add A1mAssist/codex-micro-adapter
codex plugin add codex-micro@codex-micro-adapter
```

The first interactive `codex` run shows **Hooks need review** - choose
*Trust all and continue*. Codex keeps untrusted hooks in its sandbox, where
nothing reaches the host, so the keys stay dark until you do. (For a one-off
`codex exec` there is `--dangerously-bypass-hook-trust`.)

Requirements: Node.js on `PATH` and the Codex Micro host running - start
`codex-micro-desktop`, or `codex-micro-backend run --live` for the console host.

## Which key lights up

Every hook forwards the `session_id` from the harness's own payload, and the
host hands out the agent keys itself, so six terminals need no per-shell setup.
The host answers with the key it took (`ok session 7f3a agent 3 working`): keys
go lowest-first, and when all six are taken the dullest one changes hands
(`off`, then idle, then unread, then the waiting and working states).

| variable | default | meaning |
| --- | --- | --- |
| `CODEX_MICRO_PORT` | `27700` | the host's control port |
| `CODEX_MICRO_AGENT` | unset | pin one key (0-5) instead of letting the host choose |
| `CODEX_MICRO_SESSION` | unset | override the session id the host sees |
| `CODEX_MICRO_DEBUG` | unset | append a trace of what the hook did to this file |

## Tuning the states

`hooks/hooks.json` (Claude Code) and `hooks/codex-hooks.json` (Codex CLI) are
plain hook configuration - edit the state word after the script path, or drop an
event. `report.mjs` also maps a harness event name to a status on its own, so a
harness that only has different event names still works. The vocabulary is
`off`, `idle`, `working`, `unread`, `awaiting-approval`, `awaiting-response`,
`error`, and `end` (release the key).
