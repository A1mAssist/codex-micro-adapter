# Keyboard presets

The other half of an adapter: what a Codex Micro key *does* in a harness. A
preset is just the `bindings` block of `%APPDATA%\codex-micro\config.json`, so
merge the one you want into that file (an empty map means every action is
reported in the log instead of being sent).

```json
{
  "bindings": {
    "composer.submit": "enter"
  }
}
```

Binding syntax is `mod+mod+key`, `type:<literal text>` or `url:<https url>`.
Keys can also be bound per stick direction (`stick:up`), per knob tick
(`encoder:up` / `encoder:down`) and for push to talk (`ptt`).

| Preset | Verified against | Notes |
| --- | --- | --- |
| `claude-code.json` | the keybinding table inside the installed Claude Code 2.1.x bundle | `enter` confirms, `escape` declines, `shift+tab` cycles the permission mode (older Windows terminals bind that to `alt+m` instead) |
| `pi.json` | `packages/coding-agent/docs/keybindings.md` | `enter` confirms a select/confirm dialog, `escape` cancels |
| `opencode.json` | `keybinds.mdx` | only submit is bound: opencode's approval dialog has no documented keys |

Codex CLI and DeepSeek Harness have **no preset yet**: the Codex CLI keymap has
not been verified against its own binary, and DeepSeek Harness is a Web UI whose
approval and stop controls are buttons with no key tokens at all.
