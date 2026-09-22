# Codex Micro plugin for Claude Code

Turns the six agent keys on a Work Louder Codex Micro into a live view of your
Claude Code sessions. The hooks report state over the host's loopback socket —
nothing is sent anywhere else.

| Claude Code event | Agent key shows |
| --- | --- |
| `SessionStart` | Idle (white) |
| `UserPromptSubmit`, `PreToolUse`, `SubagentStop`, `PreCompact` | Working (blue, snaking) |
| `Stop` | Unread (green) |
| `Notification` (waiting for you) | Awaiting approval (orange) |
| `SessionEnd` | Off |

## Install

```bash
claude plugin marketplace add A1mAssist/codex-micro-adapter
claude plugin install codex-micro@codex-micro-adapter
```

or, if you prefer to keep it local, copy `plugins/claude-code` into
`~/.claude/plugins/` and enable it.

Requirements: Node.js on `PATH` (the hooks are a 40-line script) and the Codex
Micro host running — start `codex-micro-desktop`, or `codex-micro-backend run
--live` for the console host.

## Which key lights up

Every hook forwards the `session_id` from Claude Code's own payload, and the
host hands out the agent keys itself — so six terminals need no per-shell
setup. The host answers with the key it took (`ok session 7f3a agent 3
working`): keys go lowest-first, and when all six are taken the dullest one
changes hands (`off`, then idle, then unread, then the waiting/working states).

| variable | default | meaning |
| --- | --- | --- |
| `CODEX_MICRO_PORT` | `27700` | the host's control port |
| `CODEX_MICRO_AGENT` | unset | pin one key (0-5) instead of letting the host choose |
| `CODEX_MICRO_SESSION` | unset | override the session id the host sees |

## Tuning the states

`hooks/hooks.json` is plain Claude Code hook configuration — edit the state word
after the script path. The vocabulary is `off`, `idle`, `working`, `unread`,
`awaiting-approval`, `awaiting-response`, `error`.
