# Codex Micro adapter for DeepSeek Harness (`dsh`)

`dsh` (MIT, developer preview) ships a Web UI and an ACP stdio server. It also
has a Codex-style hooks bridge, but that bridge drops `PermissionRequest` and
has no `SessionEnd`, which leaves two agent-key states dark. This adapter is a
**native Cordis plugin** instead: it listens on the harness seams directly and
reports the whole lifecycle. No `<REPO>` placeholder, no per-event config file.

## Install

1. Mount this folder as a plugin in the profile you actually run (repeat for
   `web` / `acp`):

   ```powershell
   npx @deepseek-ai/dsh plugin --profile web add <REPO>/plugins/deepseek/plugin
   ```

2. Add the mount to `%USERPROFILE%\.dsh\profiles\web\cordis.patch.yml`. The
   `id` is required - a bare `- name:` entry is rejected with
   `patch: id is required for non-insert patches`:

   ```yaml
   - insert:
       - id: codex-micro
         name: 'codex-micro-dsh'
         config:
           port: 27700
   ```

3. Start the host (`codex-micro-desktop`, or `codex-micro-backend run --live`),
   then `dsh web` (or `dsh --profile acp`). Every session lights its own agent
   key.

`CODEX_MICRO_PORT` overrides `config.port`, for a second host on another port.

## What lights up

| `dsh` seam | Agent key |
| --- | --- |
| `agent/created` | Idle |
| `agent/pre-step`, `tools/pre-execute` | Working |
| `approval/request` | Awaiting approval |
| `agent/turn-stopping` | Unread |
| `session/disposed` | Off, key released |

`approval/request` is a waterfall seam: this plugin watches it and calls
`next()`, so the real answerer stays in charge and a missing answerer behaves
exactly as it did before the plugin was mounted. `agent/disposed` is
deliberately not wired - compaction rebuilds the agent under the same session
id, and the key belongs to the session.

## Keyboard

`dsh` is a browser UI whose approval and stop controls are plain buttons with no
key tokens, so there is nothing honest to bind - this adapter only drives the
lights. (`Enter` submits a prompt, `Shift+Enter` inserts a newline.)

## Verify

```powershell
node scripts/check-harness-adapters.mjs   # fakes the seams, asserts the lines
```

A live turn additionally needs `DEEPSEEK_API_KEY` (or the Web Models page to
store one). Without a key the ACP server still creates the session, so `idle`
and `working` land on the host before the turn fails - that is how this adapter
was verified on a real `dsh`.
