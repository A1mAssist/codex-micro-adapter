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
claude plugin marketplace add A1mAssist/codex-micro-harness
claude plugin install codex-micro@codex-micro-harness
```

or, if you prefer to keep it local, copy `plugins/claude-code` into
`~/.claude/plugins/` and enable it.

Requirements: Node.js on `PATH` (the hooks are a 12-line script) and the Codex
Micro host running — start `codex-micro-desktop`, or `codex-micro-backend run
--live` for the console host.

## Configuration

| variable | default | meaning |
| --- | --- | --- |
| `CODEX_MICRO_AGENT` | `0` | which agent key (0-5) this session lights |
| `CODEX_MICRO_PORT` | `27700` | the host's control port |

With several terminals open, set a different `CODEX_MICRO_AGENT` per shell so
each session owns a key.

## Tuning the states

`hooks/hooks.json` is plain Claude Code hook configuration — edit the state word
after the script path. The vocabulary is `off`, `idle`, `working`, `unread`,
`awaiting-approval`, `awaiting-response`, `error`.
