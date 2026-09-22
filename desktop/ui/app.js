// Codex Micro settings surface.
//
// The catalogue, labels, defaults and dialogs mirror the app's own settings
// chunk; the numbers and option ids come from the same place, so a layout saved
// here is the layout the app would have written.

const invoke = window.__TAURI__.core.invoke;

const STATUS_COLORS = {
  working: "#304ffe",
  unread: "#00ff4c",
  idle: "#ffffff",
  "awaiting-approval": "#ff6d00",
  "awaiting-response": "#ff6d00",
  error: "#ff0033",
  off: null,
};

const STATUS_LABELS = {
  working: "Working",
  unread: "Unread",
  idle: "Idle",
  "awaiting-approval": "Awaiting approval",
  "awaiting-response": "Awaiting response",
  error: "Error",
  off: "Off",
};

// The app's keycap catalogue: legend, catalogue default, and which slots it fits.
const KEYCAPS = [
  { id: "FAST", legend: "FAST", command: "composer.toggleFastMode", label: "Toggle Fast mode" },
  { id: "APPR", legend: "APPR", command: "approval.approve", label: "Approve" },
  { id: "REJ", legend: "REJ", command: "approval.decline", label: "Reject" },
  { id: "SPLIT", legend: "FORK", command: "forkThread", label: "Fork chat" },
  { id: "MIC", legend: "MIC", named: "Push to talk", size: "double" },
  { id: "MIC1", legend: "MIC1", named: "Push to talk" },
  { id: "CODEX", legend: "CODEX", command: "composer.submit", label: "Send message" },
  { id: "BUG", legend: "BUG", command: "feedback", label: "Open feedback" },
  { id: "OAI", legend: "OAI", url: "https://developers.openai.com", label: "Open OpenAI docs" },
  { id: "TERM", legend: "TERM", command: "toggleTerminal", label: "Toggle terminal" },
  { id: "DWN", legend: "DWN", command: "copyConversationMarkdown", label: "Copy chat as Markdown" },
  { id: "DEL", legend: "DEL", command: "archiveThread", label: "Archive chat" },
  { id: "NEW", legend: "NEW", command: "newTask", label: "New chat" },
  { id: "NAV", legend: "NAV", command: "openBrowserTab", label: "Open browser tab" },
  { id: "MAGIC", legend: "MAGIC", command: "toggleThreadPin", label: "Pin or unpin chat" },
  { id: "DIFF", legend: "DIFF", command: "toggleReviewTab", label: "Toggle review" },
  { id: "PLAY", legend: "PLAY", command: "environmentAction1", label: "Run primary action" },
  { id: "GIT", legend: "GIT", command: "git.commit", label: "Commit or push" },
  { id: "BRCH", legend: "DRAFT", command: "git.createDraftPullRequest", label: "Create draft PR" },
  { id: "BRANCH", legend: "BRANCH", command: "git.createBranch", label: "Create branch" },
  { id: "MRG", legend: "MRG", command: "git.mergePullRequest", label: "Merge PR" },
  { id: "PR", legend: "PR", command: "git.createPullRequest", label: "Create PR" },
  { id: "PAINT", legend: "PAINT", command: "composer.addPhotos", label: "Add photos" },
  { id: "LAB", legend: "LAB", command: "settings", label: "Open Settings" },
  { id: "PARTY", legend: "PARTY", command: "openSideChat", label: "Open side chat" },
  { id: "TIME", legend: "TIME", command: "manageTasks", label: "Open Scheduled" },
  { id: "MIND+", legend: "MIND+", command: "composer.increaseReasoningEffort", label: "Increase reasoning effort" },
  { id: "MIND-", legend: "MIND-", command: "composer.decreaseReasoningEffort", label: "Decrease reasoning effort" },
  { id: "EMPT1", legend: "EMPT1", custom: true, label: "Assign any shortcut" },
  { id: "EMPT2", legend: "EMPT2", custom: true, label: "Assign any shortcut" },
  { id: "EMPT3", legend: "EMPT3", custom: true, label: "Assign any shortcut" },
  { id: "EMPT4", legend: "EMPT4", custom: true, label: "Assign any shortcut" },
  { id: "SETUP", legend: "SETUP", command: "settings", label: "Open Settings" },
  { id: "FOLD", legend: "FOLD", command: "openFolder", label: "Open folder" },
  { id: "UPL", legend: "UPL", command: "composer.addFiles", label: "Attach files and folders" },
  { id: "APPS", legend: "APPS", command: "openSkills", label: "Open plugins" },
  { id: "YOLO", legend: ":yolo:", text: ":yolo:", label: "Write :yolo: in the composer" },
  { id: "YEET", legend: ":yeet:", text: ":yeet:", label: "Write :yeet: in the composer" },
  { id: "EMPT5", legend: "EMPT5", custom: true, size: "double" },
];

const AUTO_OFF_OPTIONS = [
  ["off", "Off"],
  ["30-seconds", "30 seconds"],
  ["1-minute", "1 minute"],
  ["3-minutes", "3 minutes"],
  ["10-minutes", "10 minutes"],
  ["30-minutes", "30 minutes"],
  ["1-hour", "1 hour"],
];

const ENCODER_MODES = [
  ["composer-navigation", "Composer navigation"],
  ["reasoning", "Reasoning only"],
  ["conversation-scroll", "Conversation scrolling"],
  ["custom", "Custom assignments"],
];

const HARNESSES = [
  ["generic", "Any app (focused window)"],
  ["claude-code", "Claude Code"],
  ["codex-cli", "Codex CLI"],
];

const HARNESS_HINTS = {
  generic:
    "Keys type into whatever window has focus. Push state from any script with <code>codex-micro-backend send \"agent 0 working\"</code> (states: off, idle, working, unread, awaiting-approval, awaiting-response, error).",
  "codex-cli":
    "Same focused-window behaviour. Point your CLI hooks at <code>codex-micro-backend send</code> to light the agent keys — nothing else to install.",
  "claude-code":
    "Install the bundled plugin from <code>plugins/claude-code</code> in this repository:<br />" +
    "<code>claude plugin marketplace add &lt;this repo&gt;</code> then <code>claude plugin install codex-micro</code>.<br />" +
    "Its hooks report Working / Idle / Awaiting approval over the control socket.",
};

const GESTURE_LABELS = { right: "Turn right", left: "Turn left" };
const STICK_DIRECTIONS = ["up", "right", "down", "left"];
const STICK_LABELS = { up: "Up", right: "Right", down: "Down", left: "Left" };

const app = {
  config: null,
  snapshot: null,
  live: false,
  configPath: "",
};

let editing = { slotId: null, keycapId: null, action: null, text: "" };

const $ = (id) => document.getElementById(id);

function keycap(id) {
  return KEYCAPS.find((k) => k.id === id) || KEYCAPS[0];
}

function keycapsForSize(size) {
  return KEYCAPS.filter((k) => (size === "double" ? k.size === "double" : k.size !== "double"));
}

/** The action a slot performs: an explicit action wins, then the keycap default. */
function slotAction(slot) {
  if (!slot) return null;
  if (slot.action) {
    if (slot.action.type === "command") return slot.action.value;
    return null;
  }
  return slot.commandId || keycap(slot.keycapId).command || null;
}

function slotActionLabel(slot) {
  if (!slot) return "Unassigned";
  if (slot.action && slot.action.type !== "command") {
    if (slot.action.type === "composer-text") return `Insert ${slot.action.value.text}`;
    if (slot.action.type === "external-url") return `Open ${slot.action.value.url}`;
    if (slot.action.type === "push-to-talk") return "Push to talk";
    if (slot.action.type === "skill") return `Skill ${slot.action.value.skillName}`;
    return "Custom shortcut";
  }
  const command = slotAction(slot);
  if (!command) return "Assign any shortcut";
  const known = KEYCAPS.find((k) => k.command === command);
  if (known) return known.label;
  return command;
}

// ---------------------------------------------------------------- transport

async function refresh({ structure = false } = {}) {
  try {
    const status = await invoke("status");
    app.config = status.config;
    app.snapshot = status.snapshot;
    app.live = status.live;
    app.configPath = status.configPath;
    if (structure || !document.querySelector(".cell.keycap")) renderStructure();
    renderDynamic();
  } catch (err) {
    toast(String(err));
  }
}

/** Structural renders happen only after a config change, so dialogs keep focus. */
function renderStructure() {
  renderSelectOptions();
  renderChassis();
  renderHarnessHint();
}

async function saveConfig() {
  try {
    const status = await invoke("save_config", { config: app.config });
    app.config = status.config;
    app.snapshot = status.snapshot;
    renderChassis();
    renderDynamic();
  } catch (err) {
    toast(`Could not save: ${err}`);
  }
}

function renderSelectOptions() {
  fill($("auto-off"), AUTO_OFF_OPTIONS);
  fill($("encoder-mode"), ENCODER_MODES);
  fill($("harness"), HARNESSES);
}

function fill(select, options) {
  select.innerHTML = "";
  for (const [value, label] of options) {
    const option = document.createElement("option");
    option.value = value;
    option.textContent = label;
    select.append(option);
  }
}

function renderDynamic() {
  const snapshot = app.snapshot || {};
  const config = app.config || {};

  const chip = $("connection-chip");
  chip.dataset.status = snapshot.status || "not-detected";
  chip.textContent =
    {
      connected: "Connected",
      detected: "Detected",
      error: "Connection problem",
      "not-detected": "Not detected",
    }[snapshot.status] || "Not detected";

  $("connection-note").textContent = snapshot.error
    ? snapshot.error
    : snapshot.status === "connected"
      ? `${snapshot.transport === "bluetooth" ? "Bluetooth" : "USB"} connection`
      : "Looking for a Codex Micro over USB or Bluetooth.";

  $("battery-row").hidden = snapshot.battery == null;
  $("battery").textContent = snapshot.battery == null ? "" : `${snapshot.battery}%${snapshot.charging ? " · charging" : ""}`;
  $("firmware-row").hidden = !snapshot.firmware;
  $("firmware").textContent = snapshot.firmware || "";

  $("brightness").value = config.brightnessPercent ?? 100;
  setRangeFill($("brightness"));
  $("brightness-readout").textContent = `${config.brightnessPercent ?? 100}%`;
  $("auto-off").value = config.autoOff || "off";
  $("encoder-mode").value = config.layout?.encoderMode || "conversation-scroll";
  $("separate-mic").checked = Boolean(config.layout?.separateMicrophoneKeys);
  $("harness").value = config.harness || "generic";
  $("live-toggle").checked = app.live;
  $("control-port").textContent = `127.0.0.1:${config.controlPort ?? 27700}`;

  const slots = new Map((snapshot.slots || []).map((slot) => [slot.id, slot.status]));
  document.querySelectorAll(".cell.agent").forEach((cell) => {
    const status = slots.get(Number(cell.dataset.agent)) || "off";
    const color = STATUS_COLORS[status];
    const dot = cell.querySelector(".agent-dot");
    dot.style.setProperty("--status-color", color || "transparent");
    dot.style.setProperty("--status-opacity", color ? "0.55" : "0");
    cell.title = `Agent key ${Number(cell.dataset.agent) + 1}: ${STATUS_LABELS[status] || status}`;
  });

  renderAgentKeyList(slots);
  $("log").textContent = (snapshot.log || []).slice(-40).join("\n") || "Nothing yet.";
  $("knob-note").textContent = knobNote();
  renderHarnessHint();
}

function knobNote() {
  const mode = app.config?.layout?.encoderMode;
  return {
    "composer-navigation": "Move through composer controls and options",
    reasoning: "Open and adjust reasoning effort",
    "conversation-scroll": "Scroll through the active conversation",
    custom: "Choose an action for each turn (click and press-and-hold come from the binding table)",
  }[mode] || "Choose what turning the knob controls";
}

function renderAgentKeyList(slots) {
  const list = $("agent-keys");
  list.innerHTML = "";
  for (let id = 0; id < 6; id += 1) {
    const status = slots.get(id) || "off";
    const entry = document.createElement("div");
    entry.innerHTML = `<span class="swatch" style="--status-color:${STATUS_COLORS[status] || "transparent"}"></span>
      <span>Agent key ${id + 1}</span>
      <span style="margin-left:auto;color:var(--text-tertiary)">${STATUS_LABELS[status] || status}</span>`;
    list.append(entry);
  }
}

function setRangeFill(input) {
  input.style.setProperty("--fill", `${input.value}%`);
}

function renderHarnessHint() {
  const harness = app.config?.harness || "generic";
  $("harness-hint").innerHTML = HARNESS_HINTS[harness] || HARNESS_HINTS.generic;
}

// ------------------------------------------------------------ device preview

function renderChassis() {
  const layout = app.config?.layout || { slots: {} };
  const separate = Boolean(layout.separateMicrophoneKeys);
  const chassis = $("chassis");
  chassis.innerHTML = "";

  chassis.append(knobCell(), agentCell(0), agentCell(1), stickCell(), agentCell(2), agentCell(3), agentCell(4), agentCell(5));
  for (const slotId of ["ACT06", "ACT07", "ACT08", "ACT09"]) chassis.append(keycapCell(slotId));
  chassis.append(ledCell());
  if (separate) {
    chassis.append(keycapCell("ACT10"), keycapCell("ACT11"));
  } else {
    // one wide key across the two switches, exactly like the device
    chassis.append(keycapCell("ACT10_ACT11", { merged: true }));
  }
  chassis.append(keycapCell("ACT12"));
}

function keycapCell(slotId, { merged = false } = {}) {
  const layout = app.config?.layout || { slots: {} };
  const slot = layout.slots?.[slotId] || { keycapId: "" };
  const cap = keycap(slot.keycapId || "EMPT1");
  const cell = document.createElement("button");
  cell.className = "cell keycap";
  cell.dataset.slot = slotId;
  cell.style.gridColumn = merged ? "span 2" : "";
  cell.title = `${slotId} · ${slotActionLabel(slot)}`;
  cell.setAttribute("aria-label", `${slotActionLabel(slot)} on ${slotId}`);
  const plate = document.createElement("span");
  plate.className = "plate";
  plate.dataset.unassigned = String(!slot.keycapId);
  plate.textContent = cap.legend;
  cell.append(plate);
  cell.addEventListener("click", () => openKeycapDialog(slotId));
  return cell;
}

function agentCell(id) {
  const cell = document.createElement("div");
  cell.className = "cell agent";
  cell.dataset.agent = String(id);
  cell.innerHTML = '<span class="agent-dot"></span><span class="agent-legend"></span>';
  return cell;
}

function knobCell() {
  const cell = document.createElement("button");
  cell.className = "cell knob";
  cell.title = "Knob";
  cell.setAttribute("aria-label", "Configure knob actions");
  cell.innerHTML = '<span class="knob-face"></span>';
  cell.addEventListener("click", openKnobDialog);
  return cell;
}

function stickCell() {
  const cell = document.createElement("button");
  cell.className = "cell stick";
  cell.title = "Analog stick";
  cell.setAttribute("aria-label", "Configure analog stick actions");
  cell.innerHTML = '<span class="stick-housing"><span class="stick-ball"></span></span>';
  cell.addEventListener("click", openStickDialog);
  return cell;
}

function ledCell() {
  const cell = document.createElement("div");
  cell.className = "cell led";
  cell.setAttribute("aria-hidden", "true");
  cell.innerHTML =
    '<span class="led-column"><span class="led"></span><span class="led dim"></span><span class="led dim"></span></span>' +
    '<span class="led-body"></span>';
  return cell;
}

// ------------------------------------------------------------ keycap editor

function openKeycapDialog(slotId) {
  const slot = app.config.layout.slots?.[slotId] || { keycapId: "" };
  editing = { slotId, keycapId: slot.keycapId || "", action: slot.action || null, text: slot.action?.value?.text || "" };
  $("keycap-subtitle").textContent = `Choose what appears on ${slotId}`;
  $("keycap-search").value = "";
  renderKeycapGrid();
  renderActionPicker();
  $("keycap-dialog").showModal();
}

function renderKeycapGrid() {
  const slotId = editing.slotId;
  const size = slotId === "ACT10_ACT11" ? "double" : "single";
  const query = $("keycap-search").value.trim().toLowerCase();
  const grid = $("keycap-grid");
  grid.innerHTML = "";
  for (const cap of keycapsForSize(size)) {
    if (query && !`${cap.id} ${cap.legend} ${cap.label || ""}`.toLowerCase().includes(query)) continue;
    const tile = document.createElement("button");
    tile.type = "button";
    tile.className = "keycap-tile";
    tile.setAttribute("aria-pressed", String(cap.id === editing.keycapId));
    tile.innerHTML = `<span>${cap.legend}</span>`;
    tile.addEventListener("click", () => {
      editing.keycapId = cap.id;
      editing.action = actionFromOtherSlot(cap.id);
      editing.text = editing.action?.value?.text || "";
      renderKeycapGrid();
      renderActionPicker();
    });
    grid.append(tile);
  }
}

/** The app copies whatever action another slot has already bound to this keycap. */
function actionFromOtherSlot(keycapId) {
  for (const [slotId, slot] of Object.entries(app.config.layout.slots || {})) {
    if (slotId === editing.slotId) continue;
    if (slot.keycapId === keycapId && (slot.action || slot.commandId)) return slot.action || null;
  }
  return null;
}

function renderActionPicker() {
  const select = $("keycap-action");
  select.innerHTML = "";
  const defaultOption = document.createElement("option");
  defaultOption.value = "";
  defaultOption.textContent = "Use keycap default";
  select.append(defaultOption);

  const textOption = document.createElement("option");
  textOption.value = "composer-text";
  textOption.textContent = "Insert text…";
  select.append(textOption);

  const commands = new Map();
  for (const cap of KEYCAPS) if (cap.command) commands.set(cap.command, cap.label);
  for (const [command, label] of commands) {
    const option = document.createElement("option");
    option.value = `command:${command}`;
    option.textContent = label;
    select.append(option);
  }

  const current = editing.action;
  select.value = !current
    ? ""
    : current.type === "composer-text"
      ? "composer-text"
      : current.type === "command"
        ? `command:${current.value}`
        : "";

  $("keycap-text-row").hidden = select.value !== "composer-text";
  $("keycap-text").value = editing.text;
  $("keycap-action-note").textContent = select.value ? "" : (keycap(editing.keycapId).label || "Keycap default");
}

function commitKeycapEditor() {
  const layout = app.config.layout;
  const slotId = editing.slotId;
  const selectValue = $("keycap-action").value;
  const action =
    selectValue === ""
      ? null
      : selectValue === "composer-text"
        ? { type: "composer-text", value: { label: `Insert ${$("keycap-text").value}`, text: $("keycap-text").value } }
        : { type: "command", value: selectValue.slice("command:".length) };

  layout.slots = layout.slots || {};
  layout.slots[slotId] = { keycapId: editing.keycapId, commandId: null, action };

  // the app clears the same keycap from any other slot it was on
  const paired = new Set([slotId, layout.separateMicrophoneKeys ? "" : "ACT10_ACT11"]);
  for (const [other, slot] of Object.entries(layout.slots)) {
    if (paired.has(other)) continue;
    if (slot.keycapId === editing.keycapId && editing.keycapId) {
      layout.slots[other] = { keycapId: "EMPT1", commandId: null, action: null };
    }
  }
  saveConfig();
}

// --------------------------------------------------------- knob / stick maps

function actionPickerRow(label, action, onChange) {
  const row = document.createElement("div");
  row.className = "mapping-row";
  const labelEl = document.createElement("div");
  labelEl.className = "mapping-label";
  labelEl.textContent = label;
  const select = document.createElement("select");
  const none = document.createElement("option");
  none.value = "";
  none.textContent = "None";
  select.append(none);
  const commands = new Map();
  for (const cap of KEYCAPS) if (cap.command) commands.set(cap.command, cap.label);
  for (const [command, commandLabel] of commands) {
    const option = document.createElement("option");
    option.value = command;
    option.textContent = commandLabel;
    select.append(option);
  }
  select.value = action && action.type === "command" ? action.value : "";
  select.addEventListener("change", () => onChange(select.value ? { type: "command", value: select.value } : null));
  row.append(labelEl, select);
  return row;
}

function openKnobDialog() {
  const layout = app.config.layout;
  layout.encoder = layout.encoder || {};
  $("mapping-title").textContent = "Knob";
  $("mapping-desc").textContent = "Choose what each knob gesture triggers";
  const rows = $("mapping-rows");
  rows.innerHTML = "";
  for (const gesture of ["right", "left"]) {
    rows.append(
      actionPickerRow(GESTURE_LABELS[gesture], layout.encoder[gesture], (action) => {
        if (action) layout.encoder[gesture] = action;
        else delete layout.encoder[gesture];
        saveConfig();
      }),
    );
  }
  const note = document.createElement("p");
  note.className = "hint";
  note.innerHTML =
    'Click and press-and-hold are not mapped yet — bind <code>encoder:press</code> and <code>encoder:release</code> in <code>' +
    (app.configPath || "config.json") +
    "</code>, which the Knob dropdown writes for every other mode.";
  rows.append(note);
  $("mapping-dialog").showModal();
}

function openStickDialog() {
  const layout = app.config.layout;
  layout.analogStick = layout.analogStick || {};
  $("mapping-title").textContent = "Analog stick";
  $("mapping-desc").textContent = "Choose what each direction triggers";
  const rows = $("mapping-rows");
  rows.innerHTML = "";
  for (const direction of STICK_DIRECTIONS) {
    rows.append(
      actionPickerRow(STICK_LABELS[direction], layout.analogStick[direction], (action) => {
        if (action) layout.analogStick[direction] = action;
        else delete layout.analogStick[direction];
        saveConfig();
      }),
    );
  }
  $("mapping-dialog").showModal();
}

// ------------------------------------------------------------------- wiring

$("brightness").addEventListener("input", (event) => {
  setRangeFill(event.target);
  $("brightness-readout").textContent = `${event.target.value}%`;
});
$("brightness").addEventListener("change", (event) => {
  app.config.brightnessPercent = Number(event.target.value);
  saveConfig();
});

$("auto-off").addEventListener("change", (event) => {
  app.config.autoOff = event.target.value || null;
  saveConfig();
});

$("encoder-mode").addEventListener("change", (event) => {
  app.config.layout.encoderMode = event.target.value;
  if (event.target.value === "custom") {
    renderChassis();
    openKnobDialog();
  }
  saveConfig();
});

$("separate-mic").addEventListener("change", (event) => {
  app.config.layout.separateMicrophoneKeys = event.target.checked;
  renderChassis();
  saveConfig();
});

$("harness").addEventListener("change", (event) => {
  app.config.harness = event.target.value;
  renderHarnessHint();
  saveConfig();
});

$("live-toggle").addEventListener("change", async (event) => {
  const status = await invoke("set_live", { live: event.target.checked });
  app.live = status.live;
  toast(status.live ? "Keystrokes will be sent to the focused window" : "Dry run: actions are logged only");
});

$("rescan").addEventListener("click", async () => {
  await refresh();
  toast("Rescanning for the keyboard");
});

$("copy-port").addEventListener("click", async () => {
  await navigator.clipboard.writeText(`codex-micro-backend send "agent 0 working"`);
  toast("Example command copied");
});

$("keycap-search").addEventListener("input", renderKeycapGrid);
$("keycap-action").addEventListener("change", () => {
  $("keycap-text-row").hidden = $("keycap-action").value !== "composer-text";
  $("keycap-action-note").textContent = $("keycap-action").value ? "" : keycap(editing.keycapId).label || "";
});
$("keycap-form").addEventListener("submit", (event) => {
  if (event.submitter?.value === "save") commitKeycapEditor();
});

$("reset-layout").addEventListener("click", () => {
  confirmDialog(
    "Reset keyboard layout?",
    "This restores the command keys and analog stick to their default assignments without changing your agent key state.",
    "Reset layout",
    () => {
      app.config.layout = { ...app.config.layout, ...defaultLayout() };
      saveConfig();
    },
  );
});

function defaultLayout() {
  const slots = {};
  for (const [slot, cap] of [
    ["ACT06", "FAST"],
    ["ACT07", "APPR"],
    ["ACT08", "REJ"],
    ["ACT09", "SPLIT"],
    ["ACT10", "MIC1"],
    ["ACT11", "EMPT1"],
    ["ACT10_ACT11", "MIC"],
    ["ACT12", "CODEX"],
  ]) {
    slots[slot] = { keycapId: cap, commandId: null, action: null };
  }
  const analogStick = {
    up: { type: "command", value: "composer.togglePlanMode" },
    right: { type: "command", value: "navigateForward" },
    down: { type: "command", value: "toggleSidebar" },
    left: { type: "command", value: "navigateBack" },
  };
  return { slots, analogStick, encoder: {} };
}

function confirmDialog(title, body, confirmLabel, onConfirm) {
  $("confirm-title").textContent = title;
  $("confirm-body").textContent = body;
  $("confirm-ok").textContent = confirmLabel;
  const dialog = $("confirm-dialog");
  const handler = () => {
    if (dialog.returnValue === "confirm") onConfirm();
    dialog.removeEventListener("close", handler);
  };
  dialog.addEventListener("close", handler);
  dialog.showModal();
}

let toastTimer = null;
function toast(message) {
  const el = $("toast");
  el.textContent = message;
  el.hidden = false;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => {
    el.hidden = true;
  }, 2600);
}

refresh({ structure: true });
setInterval(() => refresh(), 500);
