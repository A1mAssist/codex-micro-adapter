// Codex Micro settings surface.
//
// Catalogue, option ids, defaults and dialog copy mirror the app's own settings
// chunk; the icons come from the app's icon components (see icons.js). What the
// app keeps inside its thread store — agent key sources, thread titles — is
// replaced here by the control socket, which is what the Adapter card documents.

import { iconSvg } from "./icons.js";

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

/** The app's keycap catalogue: legend, icon id, catalogue default action. */
const KEYCAPS = [
  { id: "FAST", legend: "FAST", icon: "lightning-outline", command: "composer.toggleFastMode", label: "Toggle Fast mode" },
  { id: "APPR", legend: "APPR", icon: "check-circle", command: "approval.approve", label: "Approve" },
  { id: "REJ", legend: "REJ", icon: "x-circle", command: "approval.decline", label: "Reject" },
  { id: "SPLIT", legend: "FORK", icon: "worktree", command: "forkThread", label: "Fork chat" },
  { id: "MIC", legend: "MIC", icon: "mic", named: "Push to talk", size: "double" },
  { id: "MIC1", legend: "MIC1", icon: "mic", named: "Push to talk" },
  { id: "CODEX", legend: "CODEX", icon: "codex", command: "composer.submit", label: "Send message" },
  { id: "BUG", legend: "BUG", icon: "bug", command: "feedback", label: "Open feedback" },
  { id: "OAI", legend: "OAI", icon: "openai", url: "https://developers.openai.com", label: "Open OpenAI docs" },
  { id: "TERM", legend: "TERM", icon: "terminal", command: "toggleTerminal", label: "Toggle terminal" },
  { id: "DWN", legend: "DWN", icon: "download", command: "copyConversationMarkdown", label: "Copy chat as Markdown" },
  { id: "DEL", legend: "DEL", icon: "trash", command: "archiveThread", label: "Archive chat" },
  { id: "NEW", legend: "NEW", icon: "compose", command: "newTask", label: "New chat" },
  { id: "NAV", legend: "NAV", icon: "pointer-outline", command: "openBrowserTab", label: "Open browser tab" },
  { id: "MAGIC", legend: "MAGIC", icon: "star", command: "toggleThreadPin", label: "Pin or unpin chat" },
  { id: "DIFF", legend: "DIFF", icon: "diff", command: "toggleReviewTab", label: "Toggle review" },
  { id: "PLAY", legend: "PLAY", icon: "play-outline", command: "environmentAction1", label: "Run primary action" },
  { id: "GIT", legend: "GIT", icon: "diff", command: "git.commit", label: "Commit or push" },
  { id: "BRCH", legend: "DRAFT", icon: "pull-request-draft", command: "git.createDraftPullRequest", label: "Create draft PR" },
  { id: "BRANCH", legend: "BRANCH", icon: "branch", command: "git.createBranch", label: "Create branch" },
  { id: "MRG", legend: "MRG", icon: "pull-request-merged", command: "git.mergePullRequest", label: "Merge PR" },
  { id: "PR", legend: "PR", icon: "pull-request", command: "git.createPullRequest", label: "Create PR" },
  { id: "PAINT", legend: "PAINT", icon: "paint", command: "composer.addPhotos", label: "Add photos" },
  { id: "LAB", legend: "LAB", icon: "flask", command: "settings", label: "Open Settings" },
  { id: "PARTY", legend: "PARTY", icon: "confetti", command: "openSideChat", label: "Open side chat" },
  { id: "TIME", legend: "TIME", icon: "clock", command: "manageTasks", label: "Open Scheduled" },
  { id: "MIND+", legend: "MIND+", icon: "brain-medium", command: "composer.increaseReasoningEffort", label: "Increase reasoning effort" },
  { id: "MIND-", legend: "MIND-", icon: "brain-outline", command: "composer.decreaseReasoningEffort", label: "Decrease reasoning effort" },
  { id: "EMPT1", legend: "EMPT1", icon: "empty", custom: true, label: "Assign any shortcut" },
  { id: "EMPT2", legend: "EMPT2", icon: "empty", custom: true, label: "Assign any shortcut" },
  { id: "EMPT3", legend: "EMPT3", icon: "empty", custom: true, label: "Assign any shortcut" },
  { id: "EMPT4", legend: "EMPT4", icon: "empty", custom: true, label: "Assign any shortcut" },
  { id: "SETUP", legend: "SETUP", icon: "settings", command: "settings", label: "Open Settings" },
  { id: "FOLD", legend: "FOLD", icon: "folder-plus", command: "openFolder", label: "Open folder" },
  { id: "UPL", legend: "UPL", icon: "cloud-upload", command: "composer.addFiles", label: "Attach files and folders" },
  { id: "APPS", legend: "APPS", icon: "all-products", command: "openSkills", label: "Open plugins" },
  { id: "YOLO", legend: ":yolo:", icon: "empty", text: ":yolo:", label: "Write :yolo: in the composer" },
  { id: "YEET", legend: ":yeet:", icon: "empty", text: ":yeet:", label: "Write :yeet: in the composer" },
  { id: "EMPT5", legend: "EMPT5", icon: "empty", custom: true, size: "double" },
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

// The app's row asks which store the agent keys follow. A standalone host has
// exactly one store — whatever the plugins push over the socket — so the row
// picks which client's plugin is feeding it.
const HARNESSES = [
  ["generic", "Any client (control socket)"],
  ["claude-code", "Claude Code plugin"],
  ["codex-cli", "Codex CLI hooks"],
  ["off", "Off"],
];

const HARNESS_HINTS = {
  generic:
    'Any script can drive the keys: <code>codex-micro-backend send "agent 0 working"</code>. States: off, idle, working, unread, awaiting-approval, awaiting-response, error.',
  "codex-cli":
    'Point your CLI hooks at <code>codex-micro-backend send</code>; nothing else to install. Keys type into whatever window has focus.',
  "claude-code":
    "Install the bundled plugin: <code>claude plugin marketplace add A1mAssist/codex-micro-adapter</code> then <code>claude plugin install codex-micro@codex-micro-adapter</code>.",
  off: "Agent key updates from the socket are ignored; the keys stay dark.",
};

const GESTURE_LABELS = { right: "Turn right", left: "Turn left" };
const STICK_DIRECTIONS = ["up", "right", "down", "left"];
const STICK_LABELS = { up: "Up", right: "Right", down: "Down", left: "Left" };

const app = { config: null, snapshot: null, live: false, configPath: "" };
let editing = { slotId: null, keycapId: null, action: null, text: "" };

const $ = (id) => document.getElementById(id);
const keycap = (id) => KEYCAPS.find((k) => k.id === id) || KEYCAPS[0];
const keycapsForSize = (size) => KEYCAPS.filter((k) => (size === "double" ? k.size === "double" : k.size !== "double"));

function slotAction(slot) {
  if (!slot || !slot.action || slot.action.type !== "command") return slot?.commandId || keycap(slot?.keycapId).command || null;
  return slot.action.value;
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
  return KEYCAPS.find((k) => k.command === command)?.label || command;
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

function renderStructure() {
  fill($("auto-off"), AUTO_OFF_OPTIONS);
  fill($("encoder-mode"), ENCODER_MODES);
  fill($("harness"), HARNESSES);
  fill($("mic-mode"), [["push-to-talk", "Push to talk"], ["voice-chat", "Voice Chat (ChatGPT app only)"]]);
  $("mic-mode").options[1].disabled = true;
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

  $("connection").textContent =
    {
      connected: snapshot.transport === "bluetooth" ? "Connected · Bluetooth" : "Connected",
      detected: "Detected",
      error: snapshot.error ? `Connection problem` : "Connection problem",
      "not-detected": "Not detected",
    }[snapshot.status] || "Not detected";

  $("battery-row").hidden = snapshot.battery == null;
  if (snapshot.battery != null) {
    $("battery-text").textContent = `${snapshot.battery}%`;
    $("battery-fill").setAttribute("width", String(Math.max(1, Math.round((snapshot.battery / 100) * 15))));
    $("battery-fill").setAttribute("opacity", snapshot.charging ? "0.55" : "1");
  }

  const brightness = config.brightnessPercent ?? 100;
  $("brightness").value = brightness;
  $("brightness").style.setProperty("--fill", `${brightness}%`);
  $("brightness-readout").textContent = `${brightness}%`;
  $("auto-off").value = config.autoOff || "off";
  $("encoder-mode").value = config.layout?.encoderMode || "conversation-scroll";
  $("separate-mic").checked = Boolean(config.layout?.separateMicrophoneKeys);
  $("harness").value = config.harness || "generic";
  $("live-toggle").checked = app.live;
  $("control-port").textContent = `127.0.0.1:${config.controlPort ?? 27700}`;
  $("config-path").textContent = app.configPath || "";
  $("knob-note").textContent = knobNote();

  const slots = new Map((snapshot.slots || []).map((slot) => [slot.id, slot.status]));
  for (const cell of document.querySelectorAll(".cell.agent")) {
    const status = slots.get(Number(cell.dataset.agent)) || "off";
    const color = STATUS_COLORS[status];
    const plate = cell.querySelector(".agent-plate");
    const active = color && status !== "idle";
    cell.classList.toggle("lit", Boolean(active));
    cell.style.setProperty("--lit-color", color || "transparent");
    // the app's unassigned/idle caps still show the violet centre dot
    plate.style.setProperty("--status-color", active ? "#f0efff" : "#6f63d9");
    plate.style.setProperty("--status-opacity", active ? "0.85" : "0.6");
    cell.title = `Agent key ${Number(cell.dataset.agent) + 1}: ${STATUS_LABELS[status] || status}`;
  }

  renderAgentKeyList(slots);
  $("log").textContent = (snapshot.log || []).slice(-40).join("\n") || "Nothing yet.";
  renderHarnessHint();
}

function knobNote() {
  return (
    {
      "composer-navigation": "Move through composer controls and options",
      reasoning: "Open and adjust reasoning effort",
      "conversation-scroll": "Scroll through the active conversation",
      custom: "Choose an action for each turn; click and press-and-hold use the binding table",
    }[app.config?.layout?.encoderMode] || "Choose what turning the knob controls"
  );
}

function renderAgentKeyList(slots) {
  const list = $("agent-keys");
  list.innerHTML = "";
  for (let id = 0; id < 6; id += 1) {
    const status = slots.get(id) || "off";
    const line = document.createElement("div");
    line.className = "line";
    line.innerHTML = `<span class="swatch" style="--status-color:${STATUS_COLORS[status] || "#2b2b30"}"></span>
      <span>Agent key ${id + 1}</span>
      <span class="state">${STATUS_LABELS[status] || status}</span>`;
    list.append(line);
  }
}

function renderHarnessHint() {
  const harness = app.config?.harness || "generic";
  let hint = document.querySelector(".hint");
  if (!hint) {
    hint = document.createElement("div");
    hint.className = "hint";
    $("agent-keys").after(hint);
  }
  hint.innerHTML = HARNESS_HINTS[harness] || HARNESS_HINTS.generic;
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
  if (separate) chassis.append(keycapCell("ACT10"), keycapCell("ACT11"));
  else chassis.append(keycapCell("ACT10_ACT11", { merged: true }));
  chassis.append(keycapCell("ACT12"));
}

function keycapCell(slotId, { merged = false } = {}) {
  const slot = app.config?.layout?.slots?.[slotId] || { keycapId: "" };
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
  plate.innerHTML = iconSvg(cap.icon);
  cell.append(plate);
  cell.addEventListener("click", () => openKeycapDialog(slotId));
  return cell;
}

function agentCell(id) {
  const cell = document.createElement("div");
  cell.className = "cell agent";
  cell.dataset.agent = String(id);
  cell.innerHTML = '<span class="agent-plate"></span>';
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
  cell.innerHTML = '<span class="housing"><span class="ball"></span></span>';
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
  const size = editing.slotId === "ACT10_ACT11" ? "double" : "single";
  const query = $("keycap-search").value.trim().toLowerCase();
  const grid = $("keycap-grid");
  grid.innerHTML = "";
  for (const cap of keycapsForSize(size)) {
    if (query && !`${cap.id} ${cap.legend} ${cap.label || ""}`.toLowerCase().includes(query)) continue;
    const tile = document.createElement("button");
    tile.type = "button";
    tile.className = "keycap-tile";
    tile.title = cap.label || cap.id;
    tile.setAttribute("aria-pressed", String(cap.id === editing.keycapId));
    tile.innerHTML = iconSvg(cap.icon);
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

/** The app copies whatever action another slot already bound to this keycap. */
function actionFromOtherSlot(keycapId) {
  for (const [slotId, slot] of Object.entries(app.config.layout.slots || {})) {
    if (slotId === editing.slotId || slot.keycapId !== keycapId) continue;
    if (slot.action || slot.commandId) return slot.action || null;
  }
  return null;
}

function renderActionPicker() {
  const select = $("keycap-action");
  select.innerHTML = "";
  const options = [["", "Use keycap default"], ["composer-text", "Insert text…"]];
  const commands = new Map();
  for (const cap of KEYCAPS) if (cap.command) commands.set(cap.command, cap.label);
  for (const [command, label] of commands) options.push([`command:${command}`, label]);
  for (const [value, label] of options) {
    const option = document.createElement("option");
    option.value = value;
    option.textContent = label;
    select.append(option);
  }
  const current = editing.action;
  select.value = !current ? "" : current.type === "composer-text" ? "composer-text" : current.type === "command" ? `command:${current.value}` : "";
  $("keycap-text-row").hidden = select.value !== "composer-text";
  $("keycap-text").value = editing.text;
  $("keycap-action-note").textContent = select.value ? "" : keycap(editing.keycapId).label || "";
}

function commitKeycapEditor() {
  const layout = app.config.layout;
  const slotId = editing.slotId;
  const value = $("keycap-action").value;
  const action =
    value === ""
      ? null
      : value === "composer-text"
        ? { type: "composer-text", value: { label: `Insert ${$("keycap-text").value}`, text: $("keycap-text").value } }
        : { type: "command", value: value.slice("command:".length) };

  layout.slots = layout.slots || {};
  layout.slots[slotId] = { keycapId: editing.keycapId, commandId: null, action };

  // the app clears the same keycap from any other slot it was on
  const keep = new Set([slotId, layout.separateMicrophoneKeys ? "" : "ACT10_ACT11"]);
  for (const [other, slot] of Object.entries(layout.slots)) {
    if (keep.has(other) || !editing.keycapId) continue;
    if (slot.keycapId === editing.keycapId) layout.slots[other] = { keycapId: "EMPT1", commandId: null, action: null };
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
  note.innerHTML = `Click and press-and-hold are not mapped yet — bind <code>encoder:press</code> and <code>encoder:release</code> in <code>${
    app.configPath || "config.json"
  }</code>. They exist in every knob mode.`;
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
  event.target.style.setProperty("--fill", `${event.target.value}%`);
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
  renderDynamic();
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
  await navigator.clipboard.writeText('codex-micro-backend send "agent 0 working"');
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
    "This restores the command keys and analog stick to their default assignments without changing your agent key mode or custom chat assignments",
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
  return {
    slots,
    analogStick: {
      up: { type: "command", value: "composer.togglePlanMode" },
      right: { type: "command", value: "navigateForward" },
      down: { type: "command", value: "toggleSidebar" },
      left: { type: "command", value: "navigateBack" },
    },
    encoder: {},
  };
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
