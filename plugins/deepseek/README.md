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

## Which sessions get a key

Six keys are shared with the other harnesses, so delegated subagents are
filtered out: their session header is marked (`origin: 'subagent'`, or a
`delegationDepth` above zero) and they never take a key. That is the same
marker `dsh` checks itself when it decides whether a session is a subagent.

Tapping an agent key brings that session's window forward - for `dsh` the
browser window it was last typing in - and then jumps the page to the
conversation itself. The host remembers which session the tapped key belongs to;
the plugin's browser half (`client.js`, wired in by the `dsh.client` declaration)
polls `GET /activation` on the control port and calls
`uiWorkspace.openSession()`.

That poll is the only way in: `dsh` has no per-session URL (the launch token is
accepted on `/` only, every other path answers 401) and no session-switch
shortcut. Two things worth knowing:

- The host port is fixed in `client.js`; edit `HOST` there if you run the host
  on another port.
- A tap from before the page loaded is not followed: the first answer only says
  where the sequence stands, so a stale tap cannot yank the page around.

## Keyboard

`dsh` is a browser UI whose controls are plain buttons with no key tokens, so
synthesising a keystroke has nothing to aim at. Instead the keyboard publishes an
event and the page calls the same session API `dsh`'s own UI calls:

```json
{
  "bindings": {
    "ACT06": "plugin:slash:plan",
    "ACT07": "plugin:approve",
    "ACT08": "plugin:reject",
    "ACT09": "plugin:cancel",
    "ACT12": "enter"
  }
}
```

| Event | What the page does |
| --- | --- |
| `plugin:approve` / `plugin:reject` | answers the pending approval, `allowed-once` / `rejected` |
| `plugin:cancel` | `session.cancel()` - stops the running turn; queued work stays and resumes |
| `plugin:slash:<name>` | `session.command("/<name>")` - any slash command this Host has |

`dsh` registers `plan`, `compact`, `goal`, `permission`, `export` and `feedback`
as slash commands; a name the Host does not have comes back unmatched rather than
being quietly swallowed.

Every event acts on the session the user is **looking at** - the main view has to
hold it. Work in a background session is left alone, because acting on it would
do something the user never asked for. The page picks events up by polling
`GET /events?since=N` on the host's control port, the same way it polls
`/activation` for agent-key taps; the first poll only baselines the sequence, so a
key pressed before the page loaded is not replayed.

`Enter` still submits a prompt and `Shift+Enter` inserts a newline; `ACT12` maps
to `enter` because that is a real key.

### Voice input

`dsh`'s voice input (record, transcribe, insert into the draft) is a React
component's internal state, not a service: the recording object is created inside
the composer slot and `ctx.slots` exposes only `register`, so another plugin
cannot **start** a recording or read its audio. There is therefore no `plugin:`
event for it.

What *is* a service is the recognizer seam, `ctx.speechToText`: every recording
the UI takes is handed to a registered provider as a 16 kHz mono PCM16 WAV.
[`asr-aliyun`](asr-aliyun/README.md) is such a provider - it transcribes in the
Aliyun cloud instead of with the bundled local model, so voice input works
without the SenseVoice download.

## Verify

```powershell
node scripts/check-harness-adapters.mjs   # fakes the seams, asserts the lines
```

A live turn additionally needs `DEEPSEEK_API_KEY` (or the Web Models page to
store one). Without a key the ACP server still creates the session, so `idle`
and `working` land on the host before the turn fails - that is how this adapter
was verified on a real `dsh`.
