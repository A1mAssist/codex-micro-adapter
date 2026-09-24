# Work Louder Codex Micro — wire protocol

Transcribed from the vendor sources that ship inside the ChatGPT desktop app
(`resources/app.asar` → `node_modules/@worklouder/*`, `codex-micro-service`,
`codex-micro-bridge`, `codex-micro-layout`) and cross-checked against the
implementations in `backend/src/`. Nothing here is inferred from packet captures.

## Transport

The device exposes a vendor-defined HID interface: **VID `0x303A`**, usage page
**`0xFF00`**. Both the Codex Micro (`PID 0x8360`, layout `universal`) and the
Creator Micro V2 (`0x8287`, `0x8288`) are supported; the host opens the first
matching interface it finds.

Reports are a fixed **64 bytes**:

```
write   [0] = 0x06        report id
        [1] = channel     2 = RPC, 3 = firmware/DFU
        [2] = length      1..=61
        [3..3+length]     payload
read    [1] = channel, [2] = length, [3..3+length] = payload
```

Incoming payloads are split on `\r?\n`, then reassembled: the first `{` starts a
buffer, and text keeps accumulating until the buffer parses as JSON. A rejected
parse is not an error — the remainder is simply still in flight.

## RPC envelope

```json
{"method":"sys.version","params":null,"id":1}
```

* No `jsonrpc` field. The vendor never sends one and the firmware rejects the
  abbreviated `{m, p}` request form.
* `id` is an integer below `1000`; the vendor picks it at random, we count
  upwards. Responses are matched **as strings**, so `"1"` and `1` are the same id.
* Replies may use the compact keys `i` (id), `m` (method), `p` (params) — the
  parser accepts both spellings.
* Every non-ASCII character in the request is escaped to `\uXXXX` before framing.
* One request is in flight at a time, with a **50 ms** cooldown between calls.
  The transport times out after **10 s**. The vendor's service layer wraps that
  in a **15 s** guard of its own (`L`), which only matters inside the app: this
  host drops the link and reconnects after the 10 s timeout instead.

The host answers these methods:

| method | request | result |
| --- | --- | --- |
| `sys.version` | `null` | `{"version":"0.1.37-ai-micro-idf-nimble"}`, or a bare string |
| `device.status` | `null` | `{"batteryPercentage":42,"isCharging":false,…}` |
| `v.oai.rgbcfg` | `{ambient, keys}` | `null` |
| `v.oai.thstatus` | `[{id,c,b,e,s,sk,sa}, …]` | `null` |

Notifications (device → host, same framing, no `id`):

| method | params | meaning |
| --- | --- | --- |
| `v.oai.hid` | `{"k":"ACT06","act":1,"ag":…}` | key event: `act` 1 = press, 0 = release, 2 = encoder tick |
| `v.oai.rad` | `{"a":0.25,"d":1.0}` | analog stick: `a` is turns (`0` right, `0.25` down, `0.5` left, `0.75` up), `d` is deflection 0..1 |

Encoder keys are `ENC_CW`, `ENC_CC`, `ENC_CLK`; agent keys are `AG00`–`AG05`;
keycaps sit in slots `ACT06`–`ACT12`, with `ACT10_ACT11` replacing `ACT10`/`ACT11`
when *Use separate microphone keys* is off.

## Lighting payloads

`v.oai.rgbcfg` carries the ring and the key backlight:

```json
{"keys":{"effect":1,"brightness":0.8,"speed":0.0,"magic":0,"color":3166206},
 "ambient":{"effect":2,"brightness":0.8,"speed":0.4,"magic":0,"color":3166206}}
```

`v.oai.thstatus` is one entry per agent key:

```json
[{"id":0,"c":3166206,"b":1.0,"e":2,"s":0.4,"sk":0,"sa":0}]
```

Effects: `0` off, `1` solid, `2` snake, `3` rainbow, `4` breath, `5` gradient,
`6` shallow breath. (The app also has a `lights.preview` call that uses the long
field names `backlight` / `underglow` — same idea, different spelling.)

### How the app derives what to send

Before every push, `codex-micro-service` runs three helpers. They are ported
verbatim in `backend/src/lighting.rs`:

```
$  thread_lighting(slots)  -> v.oai.thstatus
       off slot       -> effect off, brightness 0
       selected/pulsing -> breath at speed 0.4
       otherwise      -> solid at the status colour

se rgb_config(...)         -> v.oai.rgbcfg
       fleet status (`snakingAmbientStatus`) takes the ambient ring, keys go dark
       otherwise ambient = selected slot (snake while working) or the voice state
       keys mirror the ambient colour only while the selection highlight is up

ce voice_ambient(state)    -> ambient override
       recording  -> snake, colour 3050327
       processing -> snake, white
       completed  -> solid, white
```

Status palette (the app's `Ere` table): working `3166206`, unread `65356`,
idle `16777215`, awaiting approval/response `16739584`, error `16711731`, off `0`.

## Layout vocabulary

Keycap catalogue: `FAST APPR REJ SPLIT MIC MIC1 CODEX BUG OAI TERM DWN DEL NEW
NAV MAGIC DIFF PLAY GIT BRCH BRANCH MRG PR PAINT LAB PARTY TIME MIND+ MIND-
EMPT1…EMPT4 SETUP FOLD UPL APPS YOLO YEET EMPT5`. Defaults: `ACT06=FAST`,
`ACT07=APPR`, `ACT08=REJ`, `ACT09=SPLIT`, `ACT10=MIC1`, `ACT11=EMPT1`,
`ACT10_ACT11=MIC`, `ACT12=CODEX`.

A slot is a binding key in its own right: the host looks up `ACT06`…`ACT12`
before falling back to the keycap's own action, and a slot whose keycap carries
no action (a bare `ACT11`) still resolves through its own id.

Binding values are one combo, `type:<text>`, `url:<https url>`, `hold:<combo>`,
or `plugin:<event>`. A `hold:` key is stateful: the host presses it on the keycap's
press, repeats it every 100 ms while it stays down, and releases it on the
release. Harnesses that watch for auto-repeat to keep a recording alive (Claude
Code's push-to-talk) depend on that repeat. A code whose action is `ptt` - what
the `MIC` / `MIC1` keycaps resolve to - is just a key like any other: the default
binding is `hold:space`, and moving the keycap or rebinding the slot changes
nothing else. Keycap releases are delivered to the host for this reason; a
non-hold binding ignores them.

A `plugin:<event>` binding synthesises nothing: the host appends the event to a
feed a harness page polls over `GET /events?since=N`, and that harness's own
plugin acts on it. This is the door for a UI whose controls are plain buttons with
no key tokens (`dsh`'s approval panel). Event names are one short token; the first
poll of a page baselines the sequence, so an event from before it loaded is not
replayed.

Analog stick directions are `up`, `right`, `down`, `left` with a dead zone of
`0.5`; the app's default commands are `composer.togglePlanMode`,
`navigateForward`, `toggleSidebar`, `navigateBack`.

The knob (`layout.encoderMode`) is one of:

| `config.toml` value | ticks | click | press and hold |
| --- | --- | --- | --- |
| `composer-navigation` | app-internal | app-internal | open settings |
| `reasoning` | `ArrowUp` → `composer.decreaseReasoningEffort`, `ArrowDown` → `composer.increaseReasoningEffort` | app-internal | open settings |
| `conversation-scroll` | plain up/down arrows | jump to latest | open settings |
| `custom` | `layout.encoder.right` / `.left` | `layout.encoder.click` | `layout.encoder.longPress` |

The app's own "app-internal" cells act on its UI (the highlighted composer
control, the reasoning slider, the open thread). A standalone host has none of
that, so those land on bindings: a click becomes `encoder:click`, and
press-and-hold — which the app answers by opening its settings page — becomes the
`settings` command. A hold is **500 ms**, the app's own threshold (`wn` in
`codex-micro-bridge`). `custom` mode reads `layout.encoder.click` /
`.longPress` directly, exactly like the app.

## Deliberately not used

`#rpc#args#` (a legacy channel the app still writes), `fs.*`, `appmgr.*`,
`ui.*`, DFU and `permissions.*` are all app-side plumbing. A standalone host
needs none of them, so they are not implemented and not documented here.
