# Keyboard presets

The other half of an adapter: what a Codex Micro key *does* in a harness. A
preset is just the `bindings` block of `%APPDATA%\codex-micro\config.json`, so
merge the one you want into that file. An action with no binding is reported in
the log, never swallowed.

Binding syntax is `mod+mod+key` (`enter`, `escape`, `shift+tab`, `alt+.`,
`ctrl+shift+p`, `f5`), `type:<literal text>` or `url:<https url>`.

## Slots come first

Every command key is a binding of its own: `ACT06`…`ACT12`. The host looks the
slot up before the keycap's own action, so `"ACT07": "y"` makes the key that
prints APPR approve something in a CLI that wants `y` — no keycap swap, no
catalogue change. That is what lets one board serve several harnesses. The
keycap's action (`composer.submit`, push-to-talk) is only the fallback for a
slot with no binding.

`ACT10_ACT11` is the slot when *Use separate microphone keys* is off. Turn it on
and `ACT10` and `ACT11` become separate bindable slots; `ACT11` is bindable even
when it carries no keycap, so the second microphone switch is never silent.

In the desktop app this is the **Key sent to the current harness** box in the
keycap editor. Editing `config.json` by hand works exactly the same.

| Preset | Verified against |
| --- | --- |
| `claude-code.json` | the keybinding table inside the installed Claude Code 2.1.x bundle |
| `codex-cli.json` | codex-cli 0.155.0-alpha.9.2's own keymap strings + upstream source at that tag |
| `pi.json` | `packages/coding-agent/docs/keybindings.md` |
| `opencode.json` | `keybinds.mdx` |
| `dsh.json` | the installed `@deepseek-ai/dsh` approval service, not a keymap |

## Claude Code

| Micro action | Binding | What it does there |
| --- | --- | --- |
| submit | `enter` | `chat:submit` |
| approve | `enter` | confirm:yes (the dialog's highlighted option) |
| reject | `escape` | confirm:no |
| stick up / plan mode | `shift+tab` | `chat:cycleMode` - cycles normal / auto-accept / plan |

Other keys you can bind by hand: `ctrl+o` transcript, `ctrl+t` todos,
`ctrl+r` history search, `ctrl+l` clear input, `ctrl+c` interrupt, `ctrl+d`
exit, `escape` cancel. Claude's `meta+*` shortcuts (fast mode, model picker,
thinking toggle) are **unverified on Windows** - bind them only after trying.
Push-to-talk exists in Claude Code: its own keymap ships `space: voice:pushToTalk`
in the chat context, so `ACT10`, `ACT11` and `ptt` are all preset to
`hold:space`. It is a real hold - the key goes down on the press, repeats while
you keep holding it, and comes back up when you let go, which is exactly what
Claude Code's voice mode listens for.

## Codex CLI

| Micro action | Binding | What it does there |
| --- | --- | --- |
| submit | `enter` | submit the draft (steers while a turn runs) |
| approve | `y` | approve once - `a` is "approve for the session", `p` for a command prefix |
| reject | `d` | deny without running; `n` or `escape` declines *and* asks what to do instead |
| stick up / plan mode | `shift+tab` | cycle collaboration mode |
| reasoning down / up | `alt+,` / `alt+.` | lower / raise reasoning effort |

Also available: `ctrl+t` transcript, `alt+r` raw output, `ctrl+l` clear screen,
`ctrl+r` prompt history, `/model`, `/new`. Note the CLI only *shows* approval
prompts when `approval_policy` is not `never`; with `never` the `y`/`d` keys
never fire. Remap any of this in `config.toml` under
`[tui.keymap.<context>]`.

## pi

| Micro action | Binding | What it does there |
| --- | --- | --- |
| submit | `enter` | `tui.input.submit` |
| approve | `enter` | `tui.select.confirm` on extension dialogs |
| reject | `escape` | `tui.select.cancel` |
| stick up | `shift+tab` | thinking-level cycle (pi has no plan mode) |

Also: `ctrl+p` cycles models, `escape` interrupts, `ctrl+c` clears/exits,
`/new` starts a session.

## opencode

| Micro action | Binding | What it does there |
| --- | --- | --- |
| submit | `enter` | send the prompt |
| interrupt | `escape` | `session_interrupt` |

opencode's leader ( `ctrl+x` ) sequences - `ctrl+x n` new session, `ctrl+x q`
quit - are not expressible with the current one-combo binding grammar, and its
approval dialog has no documented keys, so neither is in the preset.

## Agent keys

Tapping an agent key is a real action, not a keystroke: the host brings the
window that session last reported from back to the front and marks the key as
selected. It remembers the window that had focus while the session was
**starting or working**; `unread` and `awaiting` events never overwrite it, so a
background session cannot steal another app's window.

Fallback: when no window was ever seen (a headless run, or a tab in a shared
terminal window) or Windows refuses the focus change, the tap goes through the
binding table as `agent.focus.0` … `agent.focus.5` - map those to your own
switch keys if you have a better idea for your setup.

```json
{
  "bindings": {
    "agent.focus.0": "ctrl+alt+1",
    "agent.focus.1": "ctrl+alt+2"
  }
}
```

`codex-micro-backend window` prints the window that has focus right now, and
`window --focus <hwnd>` exercises the focus call.

## Harnesses with no keys: `plugin:`

A Web UI whose controls are plain buttons has nothing to synthesise a keystroke
for. Those harnesses take the other route: a slot bound to `plugin:<event>`
publishes the event on the host's control port, and the harness's own plugin
polls `GET /events?since=N` and calls its API.

```json
{ "bindings": { "ACT07": "plugin:approve", "ACT08": "plugin:reject" } }
```

`dsh` is the one that ships this today (`presets/dsh.json`). The page answers
only the approval of the session the user is looking at - never one waiting in
the background, which the user never saw. `plugin:` events are ignored by a
harness whose plugin does not know them, so a preset is safe to merge.

## Not expressible today

- **Multi-step sequences** (`ctrl+b` then `1` for tmux, opencode's leader):
  the binding grammar is one combo, one text or one URL per action. Agent-key
  taps no longer need this - they focus the session window directly.
- **Push-to-talk on other harnesses**: only Claude Code has a voice key; the
  Micro's mic key can still drive `ptt` where a harness listens for one.
- **ChatGPT-only behaviour** (thread list, focus-thread, its command registry):
  replaced by the control socket and these bindings.
