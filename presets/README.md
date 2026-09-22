# Keyboard presets

The other half of an adapter: what a Codex Micro key *does* in a harness. A
preset is just the `bindings` block of `%APPDATA%\codex-micro\config.json`, so
merge the one you want into that file. An action with no binding is reported in
the log, never swallowed.

Binding syntax is `mod+mod+key` (`enter`, `escape`, `shift+tab`, `alt+.`,
`ctrl+shift+p`, `f5`), `type:<literal text>` or `url:<https url>`.

| Preset | Verified against |
| --- | --- |
| `claude-code.json` | the keybinding table inside the installed Claude Code 2.1.x bundle |
| `codex-cli.json` | codex-cli 0.155.0-alpha.9.2's own keymap strings + upstream source at that tag |
| `pi.json` | `packages/coding-agent/docs/keybindings.md` |
| `opencode.json` | `keybinds.mdx` |

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
Push-to-talk exists in Claude Code (`space` in the chat context) and can be
bound to `ptt`.

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

Pressing an agent key (`AG00`-`AG05`) goes through the same path as any other
slot: give the slot an action in the settings page (or `config.json`) and that
action resolves through `bindings`. So tapping key 3 can send a keystroke such as
`alt+3` to switch terminal tabs. What it cannot do is focus another program's
window - no harness exposes "switch to session N", and the host does not steal
focus.

## Not expressible today

- **Multi-step sequences** (`ctrl+b` then `1` for tmux, opencode's leader):
  the binding grammar is one combo, one text or one URL per action.
- **Push-to-talk on other harnesses**: only Claude Code has a voice key; the
  Micro's mic key can still drive `ptt` where a harness listens for one.
- **ChatGPT-only behaviour** (thread list, focus-thread, its command registry):
  replaced by the control socket and these bindings.
