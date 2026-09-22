# Codex Micro adapter for DeepSeek Harness (`dsh`)

DeepSeek Harness ships a Web UI (there is no TUI), so it is driven through its
**hooks bridge**: the `@deepseek-ai/dsh-hooks-codex` plugin runs a command on
lifecycle events and hands it the same JSON shape Claude Code and Codex CLI use
(`session_id`, `hook_event_name`, `cwd`, `turn_id`). The shared
`plugins/codex-micro/hooks/report.mjs` therefore works here unchanged.

## Install

1. Add the hooks plugin to the web profile (pin the version to your `dsh`):

   ```powershell
   npx @deepseek-ai/dsh plugin --profile web add @deepseek-ai/dsh-hooks-codex
   ```

2. Copy `dsh-hooks.json` into your `dsh` home (for example
   `C:\Users\<you>\.dsh\hooks\dsh-hooks.json`) and replace `<REPO>` with the
   absolute path to this repository, using forward slashes - for example
   `node D:/Workspaces/codex-micro-adapter/plugins/codex-micro/hooks/report.mjs`.

3. Point the plugin at that file in
   `%USERPROFILE%\.dsh\profiles\web\cordis.patch.yml`:

   ```yaml
   - insert:
       - id: hooks-codex
         name: '@deepseek-ai/dsh-hooks-codex'
         config:
           configPath: 'C:/Users/<you>/.dsh/hooks/dsh-hooks.json'
   ```

4. Start the host (`codex-micro-desktop`, or `codex-micro-backend run --live`),
   then `dsh web`. Every session lights its own agent key.

## What lights up

| `dsh` hook | Agent key |
| --- | --- |
| `SessionStart` | Idle |
| `UserPromptSubmit` | Working |
| `Stop` | Unread |

Two states have no hook in the Codex bridge: `PermissionRequest` is dropped, and
there is no `SessionEnd`, so a key keeps showing "unread" until another session
takes it. Both exist on `dsh`'s ACP surface (`session/request_permission`,
`session/close`) if you want them later.

## Keyboard

`dsh` is a browser UI whose approval and stop controls are plain buttons with no
key tokens, so there is nothing honest to bind - this adapter only drives the
lights. (`Enter` submits a prompt, `Shift+Enter` inserts a newline.)
