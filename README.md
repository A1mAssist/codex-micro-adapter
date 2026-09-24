# Codex Micro Adapter

English | [中文](README.zh-CN.md)

Use the Work Louder **Codex Micro** keyboard with any coding agent - Claude Code,
the Codex CLI, pi, opencode, DeepSeek Harness, or anything else that runs in a
window.

The ChatGPT/Codex desktop app drives this keyboard through a private front end.
This project is a standalone host that speaks the same device protocol and
carries over the same behaviour: the six agent keys, the keycaps, the analog
stick, the knob, the microphone key, the lighting derivation behind them, and
the settings page that configures all of it.

```
desktop/               Tauri app: settings surface + embedded host
backend/               Rust: HID framing, JSON-RPC, layout, lighting, actions
plugins/codex-micro/   Claude Code + Codex CLI plugin that reports session state
plugins/pi/            pi extension: harness events -> agent keys
plugins/opencode/      opencode plugin: harness events -> agent keys
plugins/deepseek/      DeepSeek Harness Cordis plugin: harness seams -> keys,
                       plus a browser half that opens the session you tapped
presets/               per-harness binding maps for the Micro keys
docs/HARNESSES.md      what each harness lights, event by event
docs/PROTOCOL.md       the wire protocol, as reverse-engineered
```

> 中文速览：`desktop` 是 Tauri 前端 + 内嵌 Rust 宿主，界面照搬 ChatGPT App 里的
> Codex Micro 设置页；`backend` 是设备协议、按键映射和灯光推导的 Rust 复刻。
> `plugins/` 里是五个 harness 的适配器（Claude Code、Codex CLI、pi、opencode、
> dsh），装法见下面的 [Harness setup](#harness-setup)。键盘按键默认只
> **记录日志**，打开界面右上角 “Send keystrokes” 才会真正模拟按键。

## What carries over

| Device part | Behaviour |
| --- | --- |
| Keycaps `ACT06`–`ACT12` | The app's own catalogue (38 caps: `FAST`, `APPR`, `REJ`, `SPLIT`, `CODEX`, `GIT`, `YOLO`, …) with the same defaults, in the same 4×4 layout |
| Agent keys `AG00`–`AG05` | Six status lights; any harness can drive them over the control socket |
| Analog stick | Four directions, same default commands (`composer.togglePlanMode`, `navigateForward`, `toggleSidebar`, `navigateBack`), dead zone `0.05` |
| Knob | The app's four modes: composer navigation, reasoning, conversation scrolling, custom assignments |
| Microphone key | Push to talk, including the merged/separate `ACT10`/`ACT11` switch logic |
| Lighting | The derivation the app runs before every push: `$` thread lighting, `se` keys+ambient, `ce` voice states, with the app's palette |
| Settings page | Connection, battery, brightness, auto-dim, layout editor, knob/analog dialogs, options — rebuilt as a Tauri window from the app's own strings |

Current ChatGPT-only logic (thread lists, "focus Codex with a single tap",
macOS Input Monitoring, the app's command registry) is intentionally replaced by
the control socket and the binding table — see [Deliberate differences](#deliberate-differences).

## Install

Windows 11 x64. Grab the installer from
[Releases](https://github.com/A1mAssist/codex-micro-adapter/releases) - the
`setup.exe` (NSIS) for a normal install, or the `.msi` if you deploy by policy.
Both bundle the host and the settings window, so there is nothing else to
install except Node.js, which the Claude Code and Codex CLI hooks use.

Then add the adapter for whichever agent you use - see
[Harness setup](#harness-setup).

## Build and run

Windows 11, Rust stable. The Tauri app needs the MSVC toolchain; this repository
ships a helper because the usual `cargo build` fails on machines whose Visual
Studio install has no desktop CRT:

```powershell
. .\scripts\msvc-env.ps1        # imports vcvars64 + clears CC/CXX
cargo run -p codex-micro-desktop
```

Console host (no window, useful for debugging):

```powershell
cargo run -p codex-micro-backend -- run           # dry run: actions are logged
cargo run -p codex-micro-backend -- run --live    # inject real keystrokes
cargo run -p codex-micro-backend -- list          # enumerate HID interfaces
cargo run -p codex-micro-backend -- listen        # read-only: print what the keyboard sends
cargo run -p codex-micro-backend -- window        # which window an agent key would focus
```

Dry run is the default everywhere: nothing reaches another window until you
enable **Send keystrokes** in the app or pass `--live` on the console.

Tests: `cargo test` — framing, RPC, layout, lighting, actions, the knob's
click/hold gestures, the device state machine and the control protocol. The
harness adapters have their own check: `node scripts/check-harness-adapters.mjs`
feeds the pi extension, the opencode plugin, the dsh plugin seams and the dsh
browser half real events and reads the lines they send.

## Harness setup

The host is harness-agnostic; each agent needs one adapter that reports its
sessions. Five ship here. Start the host first (`codex-micro-desktop`, or
`codex-micro-backend run --live` for the console one), then install the adapter
for whichever agent you use. Every adapter talks to `127.0.0.1:27700`; set
`CODEX_MICRO_PORT` to point them somewhere else.

| Harness | Adapter | Agent keys |
| --- | --- | --- |
| [Claude Code](#claude-code) | marketplace plugin | 8 hooks; `SessionEnd` releases the key |
| [Codex CLI](#codex-cli) | marketplace plugin | 7 hooks; an untrusted run never arrives |
| [pi](#pi) | extension file | extension events; shutdown releases the key |
| [opencode](#opencode) | plugin file | session + permission events |
| [dsh](#deepseek-harness-dsh) | native Cordis plugin | seams, incl. approval and session end; page navigation |

Event-to-state tables for all five: [`docs/HARNESSES.md`](docs/HARNESSES.md).

### Claude Code

```bash
claude plugin marketplace add A1mAssist/codex-micro-adapter
claude plugin install codex-micro@codex-micro-adapter
```

`SessionStart`, `UserPromptSubmit`, `PreToolUse`, `Notification`, `Stop`,
`PreCompact` and `SessionEnd` light that session's own key; the hook forwards the
`session_id` from the payload and the host hands out a free key, so six terminals
need no per-shell setup (`CODEX_MICRO_AGENT=0..5` pins one when you want it).
Needs Node.js on `PATH`. Keystrokes: [`presets/claude-code.json`](presets/claude-code.json) -
submit `enter`, approve `enter`, reject `escape`, plan mode `shift+tab`.

### Codex CLI

```powershell
codex plugin marketplace add A1mAssist/codex-micro-adapter
codex plugin add codex-micro@codex-micro-adapter
```

The first interactive run shows **Hooks need review** - choose *Trust all and
continue*. An untrusted hook runs inside Codex's sandbox where it cannot reach the
host, so the keys stay dark; `codex exec` can bypass the prompt for one run with
`--dangerously-bypass-hook-trust`. Codex has no `Notification` hook, so
`PermissionRequest` is what turns a key orange. Keystrokes:
[`presets/codex-cli.json`](presets/codex-cli.json) - approve `y`, deny `d`,
reasoning `alt+,` / `alt+.`.

### pi

pi has no hook config; its extension API is the surface.

```powershell
New-Item -ItemType Directory -Force ~\.pi\agent\extensions
Copy-Item plugins\pi\codex-micro.ts ~\.pi\agent\extensions\
```

That covers every project; `<repo>\.pi\extensions\` does one project instead
(and needs project trust). Session start, a submitted prompt, tool calls,
approval prompts and shutdown light the key. Keystrokes:
[`presets/pi.json`](presets/pi.json) - submit `enter`, approve `enter`, reject
`escape`.

### opencode

opencode runs plugins in-process, loaded from a directory:

```powershell
New-Item -ItemType Directory -Force ~\.config\opencode\plugins
Copy-Item plugins\opencode\codex-micro.ts ~\.config\opencode\plugins\
```

`.opencode\plugins\` inside a project does the same for one project.
`session.created`, `session.idle`, `permission.asked` and `session.deleted` are
mapped. Keystrokes: [`presets/opencode.json`](presets/opencode.json) - submit
`enter`, interrupt `escape`.

### DeepSeek Harness (dsh)

`dsh` also has a Codex-style hooks bridge, but that bridge drops
`PermissionRequest` and has no `SessionEnd`, so this adapter is a native Cordis
plugin instead. It is a folder, not a published package - point `dsh` at your
clone:

```powershell
npx @deepseek-ai/dsh plugin --profile web add <REPO>/plugins/deepseek/plugin
```

```yaml
# %USERPROFILE%\.dsh\profiles\web\cordis.patch.yml
- insert:
    - id: codex-micro
      name: 'codex-micro-dsh'
```

The `id` is required - `dsh` rejects a bare `- name:` entry with
`patch: id is required for non-insert patches`. Repeat for the `acp` profile if
you drive `dsh` from an editor.

The plugin's browser half is what makes an agent key **switch the conversation
in the page**, which no other harness needs: `dsh` has no per-session URL and no
switch shortcut, so the page polls the host for the tap and calls
`uiWorkspace.openSession()`. That poll is why the host has a `GET /activation`
endpoint; if you moved the host, edit the `HOST` constant in
`plugins/deepseek/plugin/client.js` to match. Full notes and limits:
[`plugins/deepseek/README.md`](plugins/deepseek/README.md).

### Anything else

Any language, any agent: writing `session <id> <status>` to the control port is
the whole protocol, and `session <id> end` releases the key. See
[Bindings and the control socket](#bindings-and-the-control-socket) below for
the command list. Agent-key taps then focus the window that session last
reported from, and a harness with no window at all can map `agent.focus.<n>`
instead.

## Bindings and the control socket

Two directions, both harness-agnostic:

**Keyboard → harness.** Keys and stick resolve to actions through
`%APPDATA%\codex-micro\config.json`:

```json
{
  "bindings": {
    "ACT06": "ctrl+shift+p",
    "ACT07": "ctrl+enter",
    "composer.submit": "enter",
    "approval.approve": "ctrl+enter",
    "forkThread": "type:/rewind",
    "OAI": "url:https://developers.openai.com"
  }
}
```

**Every command key answers to its own slot first.** `ACT06`…`ACT12` are
bindings like any other, so a key can take on whatever the harness in front you
needs without changing the keycap printed on it — the keycap's own action
(`composer.submit`, push-to-talk) is only the fallback when the slot is unbound.
That is how the same keyboard drives Codex CLI, Claude Code, pi and opencode
when they each want a different key. `ACT10_ACT11` is the slot when *Use
separate microphone keys* is off; with it on, `ACT10` and `ACT11` are separate
slots and both are bindable, including `ACT11` even when it carries no keycap.

Binding syntax is deliberately tiny: `mod+mod+key`, `type:<literal text>`,
`url:<https url>`, `hold:<combo>`, or `plugin:<event>`.

- `hold:` goes down when you press and comes back up when you let go, repeating
  while it is held the way a real keyboard does. Any slot can use it, so the
  microphone is not special: it is just a key whose default binding happens to be
  a hold. Put a `MIC` keycap on another slot and it works the same.
- `plugin:` synthesises nothing. The host publishes the event and the harness's
  own plugin calls its API - the door for a UI whose controls have no key tokens
  (`dsh`'s approval panel). It answers only the approval of the session the user
  is looking at, never one waiting in the background.

Unbound actions are reported, never swallowed. Defaults are almost empty on
purpose — inventing keymaps for someone else's tool is guessing.

**Harness → keyboard.** The host listens on `127.0.0.1:27700` for
newline-delimited commands, so any hook, script or plugin in any language can
light the agent keys:

```bash
codex-micro-backend send "agent 0 working"          # key 1 turns blue
codex-micro-backend send "agent 1 awaiting-approval" # key 2 turns orange
codex-micro-backend send "session 7f3a working"      # host picks a free key, remembers the owner
codex-micro-backend send "session 7f3a end"          # gives the key back
codex-micro-backend send "voice recording"           # ambient ring goes blue
codex-micro-backend send "brightness 40"
codex-micro-backend send "fleet error"               # whole ring, ignores per-key state
```

**Agent keys are buttons too.** Tapping agent key N brings the window that
session last reported from back to the front, and lights the key as selected the
way the app highlights the thread you switched to. The host remembers the window
that had focus while a session was *starting or working* — the user is typing
there at that moment — and never lets an `unread`/`awaiting` event overwrite it,
so a background session cannot steal another app's window. With no window known
(a headless run, or a multiplexer tab) or when Windows refuses the focus change,
the tap falls back to the `agent.focus.<n>` binding, so a tmux user can map it to
their own switch keys. `codex-micro-backend window` prints what would be focused
right now, and `window --focus <hwnd>` exercises the focus call itself.

`session <id> <status>` is what a harness plugin wants: it reports the session id
it already has and the host answers with the agent key it took
(`ok session 7f3a agent 3 working`). Keys are handed out lowest-first; when all
six are taken, the dullest one changes hands — `off`, then idle, then unread,
then the waiting/working states, oldest first inside each. A manual
`agent <n> …` takes its key back from whichever session owned it.

States: `off`, `idle`, `working`, `unread`, `awaiting-approval`,
`awaiting-response`, `error`.

`activation` answers `{"seq":N,"session":"<id>"|null}`: the last agent key the
user tapped, for a harness UI that can jump to that session. A browser cannot
open a socket, so the same answer is on `GET /activation` of the control port -
over HTTP, on the same port, for anything that can only speak to a page. Both
read a slot the device loop writes, so a poll keeps answering while the loop is
busy with USB work (a tap that predates the page is not replayed: the first
answer only says where the sequence stands).

## Deliberate differences

Ported from the app, minus the parts that only make sense inside it:

- **Agent key sources** (pinned / recent / priority chats) read the app's thread
  store. Here the six keys show whatever a plugin reports over the socket, and
  unassigned keys stay dark.
- **Composer navigation** and **reasoning** knob modes call app commands
  (`composer.*`); against another harness they resolve through the binding table
  like every other action.
- **Single-tap focus**, **Remove connection**, and the macOS **Input Monitoring**
  row have no equivalent in a standalone host.
- The knob's **click** and **press-and-hold** follow the app's own table: in
  `custom` mode they fire `layout.encoder.click` / `.longPress`, a hold in the
  built-in modes resolves to the `settings` command, and a click there is left to
  the `encoder:click` binding. `encoder:press` / `encoder:release` remain the raw
  press events for anything that wants them.
- Keyboard remapping is Windows-only for now (`SendInput` + SetupAPI HID);
  everything else is platform-neutral Rust.

## Protocol

`docs/PROTOCOL.md` documents the 64-byte HID framing, the `{method, params, id}`
JSON-RPC envelope, the `v.oai.*` methods and payload shapes, the lighting
derivation, and the discovery IDs — all transcribed from the vendor's own
sources, not guessed.

## Credits

Device protocol, keycap catalogue, layout rules and lighting derivation come from
the vendor sources shipped inside the ChatGPT desktop app (`@worklouder/*`,
`codex-micro-service`, `codex-micro-layout`, `codex-micro-settings`). This is an
independent, unaffiliated host: no OpenAI or Work Louder code is redistributed
here — only a Rust reimplementation of the interfaces it uses.

MIT licensed.
