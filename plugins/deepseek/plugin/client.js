/**
 * Browser half of the Codex Micro adapter for the DeepSeek Harness.
 *
 * Two jobs, both driven by the keyboard host's loopback port:
 *
 * 1. Tapping an agent key makes the host remember that session; this half asks
 *    which one and jumps the page to it. `dsh` has no per-session URL and no
 *    switch shortcut, so the page is the only place that can do the jumping.
 * 2. A key bound to `plugin:<event>` publishes an event instead of typing a
 *    keystroke. `dsh`'s approval controls are plain buttons with no key tokens,
 *    so synthesis has nothing to aim at - but the page can call the same API the
 *    buttons call. `approve` and `reject` answer the pending approval of the
 *    session the user is actually looking at.
 *
 * Nothing else is touched: an event this half does not know does nothing.
 */
export const name = "codex-micro-client";

/** The workspace service jumps; `uiSession` and `sessions` say what is pending. */
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
   * The session the user is looking at: the one the main view retains. Answering
   * a background session's approval would decide something the user never saw,
   * so an ambiguous page answers nothing at all.
   */
  const visibleSession = () => {
    const list = ctx.sessions.list.getSnapshot();
    const statuses = ctx.uiSession.sessionStatus.getSnapshot();
    for (const [id, status] of statuses) {
      if (status?.pendingInteraction?.kind !== "approval") continue;
      // `uiSession`'s own isMain(): the main view holds a reference to what it
      // shows, and only that session is one the user can answer for
      if ((list.byId?.[id]?.retainedBy?.mainView ?? 0) > 0) {
        return status.pendingInteraction;
      }
    }
    return undefined;
  };

  const answer = (outcome) => {
    const pending = visibleSession();
    if (!pending) return false;
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
  const act = (event) => {
    switch (event) {
      case "approve":
        return answer("allowed-once");
      case "reject":
        return answer("rejected");
      default:
        // An event this half does not know is ignored on purpose: another
        // plugin (or a later version) may own it.
        return false;
    }
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
      // first poll: this page has not seen anything yet, so it must not answer a
      // request that was tapped before it loaded
      seenEvents = feed.seq;
      return;
    }
    seenEvents = feed.seq;
    for (const event of feed.events ?? []) act(event);
  };

  const timer = setInterval(tick, POLL_MS);
  tick();
  ctx.effect(() => () => clearInterval(timer));
}