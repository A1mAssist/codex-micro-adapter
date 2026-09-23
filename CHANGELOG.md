# Changelog

## 0.1.0 - 2026-09-24

First release: a standalone host for the Work Louder Codex Micro keyboard, with
one adapter for each of five coding agents.

### The host

- Rust backend that speaks the keyboard's own 64-byte HID framing and JSON-RPC
  envelope, transcribed from the vendor sources rather than guessed - see
  `docs/PROTOCOL.md`.
- The app's behaviour, carried over: the keycap catalogue, the 4x4 layout, the
  analog stick, all four knob modes, the microphone key including the
  merged/separate switch, and the lighting derivation behind them.
- Six agent keys driven by anything that can write `session <id> <status>` to
  the loopback control socket. The host hands out keys lowest-first and, when all
  six are taken, takes the dullest one (`off`, then idle, then unread, then the
  waiting and working states).
- Tapping an agent key brings the window that session last reported from back to
  the front, and falls back to the `agent.focus.<n>` binding when no window is
  known.
- Dry run everywhere: keystrokes reach another window only after you turn on
  **Send keystrokes**, or pass `--live` on the console.

### The desktop app

- Tauri window rebuilt from the vendor's own settings strings: connection,
  battery, brightness, auto-dim, a layout editor for all four keycap slots, the
  knob and analog dialogs, and the adapter's own card.
- The settings route of the installed ChatGPT app can be opened side by side for
  comparison.
- About sheet with the build version, device state, firmware, control port and
  config path, plus **Copy diagnostics** for issue reports.
- Title-bar status pill, and the agent-key list sits beside the keyboard preview
  instead of below the fold.

### Harness adapters

- **Claude Code** and **Codex CLI** - one plugin, two manifests, eight and seven
  hooks. Codex asks you to trust the hooks on first run; an untrusted hook runs
  sandboxed and never reaches the host.
- **pi** - an extension file, since pi has no hook config.
- **opencode** - an in-process plugin file.
- **DeepSeek Harness** - a native Cordis plugin. The official Codex-style hooks
  bridge drops `PermissionRequest` and has no `SessionEnd`, so this one watches
  the harness seams directly, and its browser half makes an agent-key tap switch
  the conversation in the page.
- Per-harness keystroke maps in `presets/`, each verified against the tool's own
  keybinding table.

### Checks

- `cargo test` covers framing, RPC, layout, lighting, actions, the knob gestures,
  the device state machine and the control protocol.
- `node scripts/check-harness-adapters.mjs` feeds the pi extension, the opencode
  plugin, the dsh seams and the dsh browser half real events and reads what they
  send.
