/**
 * Browser half of the Codex Micro adapter for the DeepSeek Harness.
 *
 * Tapping an agent key makes the keyboard host remember that session. This half
 * asks the host which session that was and jumps the page to it — `dsh` has no
 * per-session URL and no switch shortcut, so the page is the only place that
 * can do the jumping.
 */
export const name = "codex-micro-client";

/** Jumping is the whole job; the workspace service owns it. */
export const inject = ["uiWorkspace"];

/**
 * The host is a loopback socket with one HTTP endpoint. Polling it is what
 * keeps the keyboard from needing a socket *into* the page.
 * ponytail: fixed port, make it configurable when someone runs the host
 * somewhere else.
 */
const HOST = "http://127.0.0.1:27700/activation";
const POLL_MS = 1000;

export function apply(ctx) {
  // The first answer only says where the sequence stands: a tap from before
  // this page loaded is not ours to follow.
  let seen = -1;

  const tick = async () => {
    let state;
    try {
      state = await (await fetch(HOST)).json();
    } catch {
      return; // no host listening: nothing to follow
    }
    if (!Number.isFinite(state?.seq)) return;
    if (seen < 0) {
      seen = state.seq;
      return;
    }
    if (state.seq > seen) {
      seen = state.seq;
      if (state.session) {
        try {
          ctx.uiWorkspace.openSession(state.session);
        } catch {
          // a session that has since been archived is not worth breaking the
          // poll over; the next tap gets a fresh answer
        }
      }
    }
  };

  const timer = setInterval(tick, POLL_MS);
  tick();
  ctx.effect(() => () => clearInterval(timer));
}
