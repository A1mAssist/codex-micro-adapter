# Codex Micro Harness

Use the Work Louder **Codex Micro** keyboard with any coding agent — Claude Code,
the Codex CLI, Cursor, or anything else that runs in a window.

The ChatGPT/Codex desktop app drives this keyboard through a private front end.
This project is a standalone host that speaks the same device protocol and
carries over the same behaviour: the six agent keys, the keycaps, the analog
stick, the knob, the microphone key, the lighting derivation behind them, and
the settings page that configures all of it.

```
desktop/           Tauri app: settings surface + embedded host
backend/           Rust: HID framing, JSON-RPC, layout, lighting, actions
plugins/claude-code/   Claude Code plugin that reports session state
docs/PROTOCOL.md   the wire protocol, as reverse-engineered
```

> 中文速览：`desktop` 是 Tauri 前端 + 内嵌 Rust 宿主，界面照搬 ChatGPT App 里的
> Codex Micro 设置页；`backend` 是设备协议、按键映射和灯光推导的 Rust 复刻；
> `plugins/claude-code` 是把 Claude Code 状态推给键盘的插件。键盘按键默认只
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
```

Dry run is the default everywhere: nothing reaches another window until you
enable **Send keystrokes** in the app or pass `--live` on the console.

Tests: `cargo test` (55 tests covering framing, RPC, layout, lighting, actions,
the device state machine and the control protocol).

## Pointing it at another harness

Two directions, both harness-agnostic:

**Keyboard → harness.** Keys and stick resolve to actions through
`%APPDATA%\codex-micro\config.json`:

```json
{
  "bindings": {
    "composer.submit": "enter",
    "approval.approve": "ctrl+enter",
    "forkThread": "type:/rewind",
    "OAI": "url:https://developers.openai.com"
  }
}
```

Binding syntax is deliberately tiny: `mod+mod+key`, `type:<literal text>`, or
`url:<https url>`. Unbound actions are reported, never swallowed. Defaults are
almost empty on purpose — inventing keymaps for someone else's tool is guessing.

**Harness → keyboard.** The host listens on `127.0.0.1:27700` for
newline-delimited commands, so any hook, script or plugin in any language can
light the agent keys:

```bash
codex-micro-backend send "agent 0 working"          # key 1 turns blue
codex-micro-backend send "agent 1 awaiting-approval" # key 2 turns orange
codex-micro-backend send "voice recording"           # ambient ring goes blue
codex-micro-backend send "brightness 40"
codex-micro-backend send "fleet error"               # whole ring, ignores per-key state
```

States: `off`, `idle`, `working`, `unread`, `awaiting-approval`,
`awaiting-response`, `error`. See [`plugins/claude-code`](plugins/claude-code)
for a working example that maps Claude Code hook events onto them.

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
- The knob's **click** and **press-and-hold** gestures are not part of the
  layout map yet; bind `encoder:press` / `encoder:release` instead.
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
