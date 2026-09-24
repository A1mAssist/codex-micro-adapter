/**
 * Browser half of the Codex Micro adapter for the DeepSeek Harness.
 *
 * Two jobs, both driven by the keyboard host's loopback port:
 *
 * 1. Tapping an agent key makes the host remember that session; this half asks
 *    which one and jumps the page to it. `dsh` has no per-session URL and no
 *    switch shortcut, so the page is the only place that can do the jumping.
 * 2. A key bound to `plugin:<event>` publishes an event instead of typing a
 *    keystroke, and this half calls the same API `dsh`'s own UI calls. That is
 *    the whole point: a Web UI's controls have no key tokens, so there is nothing
 *    to type at, but there is a real session API to call.
 *
 * Events this half understands:
 *
 *   approve        answer the pending approval with `allowed-once`
 *   reject         answer it with `rejected`
 *   cancel         stop the running turn
 *   slash:<name>   run the slash command `/name`
 *
 * A single session API per event, taken from the session the user is looking at.
 * Nothing else is touched, and an event this half does not know does nothing.
 */
export const name = "codex-micro-client";

/** The workspace service jumps; `uiSession` says what is pending; `sessions` acts. */
export const inject = ["uiWorkspace", "uiSession", "sessions"];

/**
 * The host is a loopback socket with two HTTP endpoints. Polling is what keeps
 * the keyboard from needing a socket *into* the page.
 * ponytail: fixed port, make it configurable when someone runs the host
 * somewhere else.
 */
const HOST = "http://127.0.0.1:27700";
const POLL_MS = 1000;

export function apply(ctx) {
  // The first answer only says where the sequence stands: a tap (or a key event)
  // from before this page loaded is not ours to act on.
  let seenActivation = -1;
  let seenEvents = -1;

  /**
   * The session the user is looking at: the one the main view retains. Acting on
   * a background session would do something the user never asked for, so an
   * ambiguous page does nothing at all. This is `uiSession`'s own `isMain` test.
   */
  const currentId = () => {
    const list = ctx.sessions.list.getSnapshot();
    for (const id of list.ids ?? []) {
      if ((list.byId?.[id]?.retainedBy?.mainView ?? 0) > 0) return id;
    }
    return undefined;
  };

  /** The live session face, or undefined while that session is not materialized. */
  const face = () => {
    const id = currentId();
    return id === undefined ? undefined : ctx.sessions.binding(id)?.session;
  };

  const answerApproval = (outcome) => {
    const id = currentId();
    const status = id === undefined
      ? undefined
      : ctx.uiSession.sessionStatus.getSnapshot().get(id);
    const pending = status?.pendingInteraction;
    if (pending?.kind !== "approval") return false;
    try {
      // the same object the approval panel resolves
      pending.answer(outcome);
      return true;
    } catch {
      // already settled by the time we looked: nothing to do
      return false;
    }
  };

  /** What a `plugin:<event>` binding means to this page. */
  const act = async (event) => {
    if (event === "approve") return answerApproval("allowed-once");
    if (event === "reject") return answerApproval("rejected");

    const live = face();
    if (!live) return false;

    if (event === "cancel") {
      // stop the running turn; queued work stays and resumes after quiescence
      await live.cancel();
      return true;
    }
    if (event.startsWith("slash:")) {
      const name = event.slice("slash:".length);
      if (!name) return false;
      // `matched:false` means this Host has no such command - the caller sees a
      // failure rather than a silently swallowed key
      const result = await live.command(`/${name}`);
      return result?.ok === true && result.value?.matched === true;
    }
    // An event this half does not know is ignored on purpose: another plugin (or
    // a later version) may own it.
    return false;
  };

  const tick = async () => {
    // --- agent-key taps: follow the session the user tapped ---
    let state;
    try {
      state = await (await fetch(`${HOST}/activation`)).json();
    } catch {
      return; // no host listening: nothing to follow
    }
    if (!Number.isFinite(state?.seq)) return;
    if (seenActivation < 0) {
      seenActivation = state.seq;
    } else if (state.seq > seenActivation) {
      seenActivation = state.seq;
      if (state.session) {
        try {
          ctx.uiWorkspace.openSession(state.session);
        } catch {
          // a session that has since been archived is not worth breaking the
          // poll over; the next tap gets a fresh answer
        }
      }
    }

    // --- key events: do what the bound key asked for ---
    let feed;
    try {
      feed = await (await fetch(`${HOST}/events?since=${seenEvents}`)).json();
    } catch {
      return;
    }
    if (!Number.isFinite(feed?.seq)) return;
    if (seenEvents < 0) {
      // first poll: this page has not seen anything yet, so it must not act on a
      // key that was pressed before it loaded
      seenEvents = feed.seq;
      return;
    }
    seenEvents = feed.seq;
    for (const event of feed.events ?? []) {
      try {
        await act(event);
      } catch {
        // one key that could not be carried out must not stop the next one
      }
    }
  };

  const timer = setInterval(tick, POLL_MS);
  tick();
  ctx.effect(() => () => clearInterval(timer));
}