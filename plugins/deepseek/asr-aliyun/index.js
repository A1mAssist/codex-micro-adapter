/**
 * Aliyun DashScope speech recognition as a DeepSeek Harness provider.
 *
 * dsh's own microphone is a browser component whose recording lives in React
 * state, so no other plugin can start it. What dsh does expose is the recognizer
 * seam, `ctx.speechToText.register()`: every recording it takes is handed to a
 * provider as a 16 kHz mono PCM16 WAV, and the provider returns text. This is
 * that provider, backed by Aliyun DashScope, which is also why the keys live in
 * `ctx.credentials` — the harness's own store, not this plugin's.
 *
 * The upstream protocol is OpenAI-Realtime shaped and was transcribed from a
 * working client rather than guessed: `session.update`, one
 * `input_audio_buffer.append` per chunk, then `session.finish`; the text arrives
 * on `conversation.item.input_audio_transcription.completed`. Server-side VAD
 * splits a recording at natural pauses, so a long recording yields several
 * completed events and they are concatenated in arrival order.
 */
export const name = "dsh-asr-aliyun";

/** The registry we register into, and the store the key comes from. */
export const inject = ["speechToText", "credentials"];

const MODEL = "qwen3-asr-flash-realtime";
/** How long the upstream may take to accept `session.update`. */
const READY_TIMEOUT_MS = 8000;
/** How long the tail may take after `session.finish`. */
const TAIL_TIMEOUT_MS = 15000;
/** One append carries 100 ms of 16 kHz mono PCM16. */
const CHUNK_BYTES = 3200;

/**
 * Languages advertised to the harness UI.
 *
 * DashScope recognises more than this; `languages` is what the picker offers, so
 * a narrow list is a conservative claim rather than a limit on the service. Add
 * a tag here when you actually use it.
 */
const LANGUAGES = ["auto", "zh", "en"];

function endpoint(workspaceId) {
  return `wss://${workspaceId}.cn-beijing.maas.aliyuncs.com/api-ws/v1/realtime?model=${MODEL}`;
}

/**
 * The PCM inside a WAV recording.
 *
 * Walking the chunks costs the same as trusting a 44-byte header and survives a
 * `LIST` chunk some encoders add, so it walks. The format is read rather than
 * assumed, because DashScope wants 16 kHz mono PCM16 and the error it gives for
 * anything else is not worth decoding.
 */
export function pcmFromWave(bytes) {
  const buf = Buffer.from(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  if (
    buf.length < 44 ||
    buf.toString("ascii", 0, 4) !== "RIFF" ||
    buf.toString("ascii", 8, 12) !== "WAVE"
  ) {
    throw new Error("the recording is not a WAV file");
  }
  let format;
  let offset = 12;
  while (offset + 8 <= buf.length) {
    const id = buf.toString("ascii", offset, offset + 4);
    const size = buf.readUInt32LE(offset + 4);
    const body = offset + 8;
    if (id === "fmt " && size >= 16) {
      format = {
        encoding: buf.readUInt16LE(body),
        channels: buf.readUInt16LE(body + 2),
        sampleRate: buf.readUInt32LE(body + 4),
        bits: buf.readUInt16LE(body + 14),
      };
    } else if (id === "data") {
      return { pcm: buf.subarray(body, Math.min(body + size, buf.length)), format };
    }
    offset = body + size + (size % 2);
  }
  throw new Error("the recording has no audio data");
}

export function requireCanonical(format) {
  if (!format) throw new Error("the recording has no format header");
  if (format.encoding !== 1 || format.bits !== 16) {
    throw new Error("the recording must be uncompressed 16-bit PCM");
  }
  if (format.channels !== 1) throw new Error("the recording must be mono");
  if (format.sampleRate !== 16000) throw new Error("the recording must be 16 kHz");
}

/** One upstream error, as the user should read it. */
function upstreamError(message) {
  return message?.error?.message ?? message?.message ?? "the recognizer reported an error";
}

/**
 * Put the socket's JSON messages behind an await, in order.
 *
 * Sequential await is all this protocol needs: one request, a few appends, then
 * a tail of completions. A push-based event emitter would be more machinery for
 * the same shape.
 */
function inbox(socket, signal) {
  const buffered = [];
  const waiting = [];
  let failure;

  /** Fail every pending and future wait with the same reason. */
  const stop = (error) => {
    failure ??= error;
    for (const waiter of waiting.splice(0)) {
      clearTimeout(waiter.timer);
      waiter.reject(error);
    }
  };
  const onAbort = () => stop(new Error("the recording was cancelled"));
  if (signal?.aborted) onAbort();
  else signal?.addEventListener("abort", onAbort, { once: true });

  socket.onmessage = (event) => {
    let parsed;
    try {
      parsed = JSON.parse(typeof event.data === "string" ? event.data : Buffer.from(event.data).toString("utf8"));
    } catch {
      return; // not ours to interpret
    }
    const waiter = waiting.shift();
    if (waiter) {
      clearTimeout(waiter.timer);
      waiter.resolve(parsed);
    } else buffered.push(parsed);
  };

  return {
    /** Await the next message, giving up on this one after `timeoutMs`. */
    next: (timeoutMs) =>
      new Promise((resolve, reject) => {
        if (failure) {
          reject(failure);
          return;
        }
        if (buffered.length > 0) {
          resolve(buffered.shift());
          return;
        }
        const waiter = { resolve, reject, timer: undefined };
        waiter.timer = setTimeout(() => {
          waiting.splice(waiting.indexOf(waiter), 1);
          reject(new Error(`the recognizer stopped answering after ${timeoutMs} ms`));
        }, timeoutMs);
        waiting.push(waiter);
      }),
    /** Stop listening for the abort; the caller owns the socket's lifetime. */
    stop: () => signal?.removeEventListener("abort", onAbort),
  };
}

function connected(connect, url, headers, signal) {
  return new Promise((resolve, reject) => {
    const socket = connect(url, { headers });
    const settle = (fn, value) => {
      signal?.removeEventListener("abort", onAbort);
      fn(value);
    };
    // A cancelled recording must not be left holding a socket open: close it and
    // reject, so the harness's unload does not wait on a connection it abandoned.
    const onAbort = () => {
      try {
        socket.close();
      } catch {
        // a socket already gone is not an error worth reporting
      }
      reject(new Error("the recording was cancelled"));
    };
    if (signal?.aborted) {
      onAbort();
      return;
    }
    signal?.addEventListener("abort", onAbort, { once: true });
    socket.onerror = () => settle(reject, new Error("could not reach the recognizer"));
    socket.onopen = () => settle(resolve, socket);
  });
}

/**
 * One recording to text.
 *
 * `signal` is the harness's cancellation: an aborted recording stops waiting and
 * closes the socket rather than leaving the upstream to finish alone.
 */
export async function recognize({ pcm, language, apiKey, workspaceId, signal, connect }) {
  if (signal?.aborted) throw new Error("the recording was cancelled");

  const socket = await connected(
    connect,
    endpoint(workspaceId),
    {
      Authorization: `Bearer ${apiKey}`,
      "OpenAI-Beta": "realtime=v1",
    },
    signal,
  );
  const next = inbox(socket, signal);
  const send = (payload) => socket.send(JSON.stringify(payload));

  try {
    send({
      event_id: `event_${crypto.randomUUID().replaceAll("-", "")}`,
      type: "session.update",
      session: {
        modalities: ["text"],
        input_audio_format: "pcm",
        sample_rate: 16000,
        // An empty object means "detect it"; `auto` is the harness's word, not
        // the service's.
        input_audio_transcription: language && language !== "auto" ? { language } : {},
        turn_detection: { type: "server_vad", threshold: 0.0, silence_duration_ms: 500 },
      },
    });

    // The session is not live until the upstream says so; appending before this
    // is dropped. Read with the ready budget so a silent upstream fails here
    // rather than waiting out the longer tail timeout.
    const readyDeadline = Date.now() + READY_TIMEOUT_MS;
    for (;;) {
      const remaining = readyDeadline - Date.now();
      if (remaining <= 0) throw new Error("the recognizer did not accept the session");
      const message = await next.next(remaining);
      if (message.type === "session.updated") break;
      if (message.type === "error") throw new Error(upstreamError(message));
    }

    for (let offset = 0; offset < pcm.length; offset += CHUNK_BYTES) {
      send({
        event_id: `event_${crypto.randomUUID().replaceAll("-", "")}`,
        type: "input_audio_buffer.append",
        audio: pcm.subarray(offset, offset + CHUNK_BYTES).toString("base64"),
      });
    }
    send({
      event_id: `event_${crypto.randomUUID().replaceAll("-", "")}`,
      type: "session.finish",
    });

    // Server VAD may split one recording into several utterances; they arrive in
    // order and read as one answer.
    // ponytail: joined with no separator, which is right for Chinese and can run
    // two English clauses together. Make it language-aware only if that is seen.
    const parts = [];
    for (;;) {
      const message = await next.next(TAIL_TIMEOUT_MS);
      switch (message.type) {
        case "conversation.item.input_audio_transcription.completed":
          if (message.transcript) parts.push(message.transcript);
          break;
        case "conversation.item.input_audio_transcription.failed":
          throw new Error(upstreamError(message));
        case "error":
          throw new Error(upstreamError(message));
        case "session.finished":
          return parts.join("");
        default:
          break; // speech_started / speech_stopped / interim text: nothing to do
      }
    }
  } finally {
    next.stop();
    try {
      socket.close();
    } catch {
      // a socket already gone is not an error worth reporting
    }
  }
}

// Defaults, not a `Config` export: cordis only treats `Config` as a schema and
// would call `["~standard"].validate` on it, so a plain object there makes the
// plugin fail to load. With no export, the harness passes `config` through as-is.
const DEFAULTS = {
  providerId: "aliyun-dashscope",
  apiKeyRef: "ALIYUN_DASHSCOPE_API_KEY",
  workspaceId: "",
};

function settingsOf(config) {
  const settings = { ...DEFAULTS, ...(config ?? {}) };
  if (!settings.workspaceId) {
    throw new Error(
      "dsh-asr-aliyun: workspaceId is required — it is the subdomain in your DashScope endpoint, not a secret",
    );
  }
  if (typeof settings.apiKeyRef !== "string" || settings.apiKeyRef.length === 0) {
    throw new Error("dsh-asr-aliyun: apiKeyRef must name a credential, such as ALIYUN_DASHSCOPE_API_KEY");
  }
  settings.connect = settings.connect ?? ((url, options) => new WebSocket(url, options));
  return settings;
}

export function apply(ctx, config) {
  const settings = settingsOf(config);

  ctx.effect(() => {
    const unregister = ctx.speechToText.register({
      info: {
        id: settings.providerId,
        name: "Aliyun DashScope",
        location: "cloud",
        languages: LANGUAGES,
      },
      transcribe: async (input, signal) => {
        const started = Date.now();
        const { pcm, format } = pcmFromWave(input.audio);
        requireCanonical(format);

        // Resolved per recording, so a rotated key applies to the next one
        // without a restart - the same reason the harness does not cache it.
        const resolved = await ctx.credentials.resolve(settings.apiKeyRef);
        if (!resolved?.value) {
          throw new Error(`no credential is set for ${settings.apiKeyRef}`);
        }

        const text = await recognize({
          pcm,
          language: input.language,
          apiKey: resolved.value,
          workspaceId: settings.workspaceId,
          signal,
          connect: settings.connect,
        });
        return {
          text,
          audioSeconds: pcm.length / 32000,
          inferenceSeconds: (Date.now() - started) / 1000,
        };
      },
    });
    return () => unregister();
  });

  ctx.logger?.info?.(`dsh-asr-aliyun: registered ${settings.providerId} (${settings.workspaceId})`);
}