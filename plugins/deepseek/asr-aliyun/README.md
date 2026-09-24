# `dsh-asr-aliyun`

Aliyun DashScope realtime speech recognition as a `dsh` speech provider. It
plugs into the harness's own microphone so the voice button transcribes in the
cloud instead of with the local SenseVoice model - no model download, no
`sherpa-onnx`.

The provider registers with `ctx.speechToText` and reads its key through
`ctx.credentials`, so the key lives in the harness store like every other
secret. The protocol (`session.update` -> `input_audio_buffer.append` ->
`session.finish`, text on
`conversation.item.input_audio_transcription.completed`) was transcribed from a
working DashScope client, not guessed.

## What it needs

- A DashScope API key (an `sk-...` from the Aliyun console).
- Your **workspace id**: the subdomain in the endpoint
  `wss://<workspaceId>.cn-beijing.maas.aliyuncs.com/...`. It is not a secret;
  it is just the workspace the key belongs to.

## Mount it

The shipped `dsh` profiles leave voice input **disabled**, so a profile patch
has to add the three voice components and this provider. Append to
`%USERPROFILE%\.dsh\profiles\<profile>\cordis.patch.yml` (this is the same shape
the official `@deepseek-ai/dsh-experimental-voice-input-bundle` uses, minus the
local model):

```yaml
- insert:
    - id: speech-to-text
      name: '@deepseek-ai/dsh-experimental-speech-to-text'
      config:
        defaultProvider: aliyun-dashscope
    - id: api-speech-to-text
      name: '@deepseek-ai/dsh-experimental-api-speech-to-text'
    - id: ui-voice-input
      name: '@deepseek-ai/dsh-experimental-client-ui-voice-input'
    - id: aliyun-asr
      name: 'dsh-asr-aliyun'
      config:
        apiKeyRef: ALIYUN_DASHSCOPE_API_KEY
        workspaceId: '<your workspace id>'
```

Install the plugin into the profile first, or point a `link:` dependency at this
folder:

```powershell
npx @deepseek-ai/dsh plugin --profile web add "link:<repo>/plugins/deepseek/asr-aliyun"
```

`ui-voice-input` is the microphone in the composer. Without it the provider is
registered but nothing calls it.

## The key

The provider resolves `apiKeyRef` on **every** recording, so a rotated key
applies to the next one without a restart. Store it once, by reference:

```powershell
$env:ALIYUN_DASHSCOPE_API_KEY='sk-...'; dsh web   # env wins for this run
```

or let it persist in `%USERPROFILE%\.dsh\.credentials.yaml`:

```yaml
version: 1
refs:
  ALIYUN_DASHSCOPE_API_KEY: sk-...
```

Launch environment > that file > the project `.env` > `$DSH_HOME/.env` is the
harness's own precedence, so nothing here has to reimplement it.

## Config

| Field | Default | Meaning |
| --- | --- | --- |
| `providerId` | `aliyun-dashscope` | id it registers under; must match `defaultProvider` above |
| `apiKeyRef` | `ALIYUN_DASHSCOPE_API_KEY` | credential **name**, never the key itself |
| `workspaceId` | - | required; the subdomain from the endpoint |
| `connect` | `new WebSocket` | test seam only |

There is deliberately no exported `Config`: in cordis that name is a schema
slot, and exporting a plain object there fails the plugin at load with
`Cannot read properties of undefined (reading 'validate')`. Defaults live
inside the module.

## Languages

`auto`, `zh`, `en`. DashScope recognises more; `languages` is only what the
picker offers, so widen `LANGUAGES` in `index.js` when you actually use a tag.

## Verify

```powershell
node scripts/check-harness-adapters.mjs   # protocol, WAV reader, config guard
```

The checks run the provider against a scripted socket - no network and no key -
and assert the frame order, the 100 ms append size, the joined multi-utterance
transcript, upstream error surfacing, bad-audio rejection, `LIST`-chunk
tolerance, and that `Config` is not exported.
