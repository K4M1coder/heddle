/**
 * DOM glue for the three screens, and nothing else.
 *
 * Every decision this file could make has been moved into `chatState.ts`,
 * `codeState.ts` and `settingsState.ts` so it can be unit-tested without a
 * browser: this module reads those states, paints them, and forwards user
 * actions to the six Tauri commands. Three of them are the ACP calls the CLI
 * already serves (`session/new`, `session/prompt`, `session/cancel`); two are
 * the reads `fs_list`/`fs_read` already perform through the same
 * `heddle_connectors::FsRoot`; one reports the flags the running child was
 * launched with. `docs/UI.md` holds the full mapping.
 *
 * The three screens share **one** session. A tab is a repaint: nothing here
 * starts, stops or re-launches an agent.
 */

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  breadcrumbs,
  childPath,
  directoryFailed,
  directoryLoaded,
  fileFailed,
  fileLoaded,
  fileOpening,
  loadingDirectory,
  parentOf,
  initialState as initialCodeState,
  type CodeState,
  type DirEntry,
} from "./codeState";
import {
  rootLabel,
  settingsRows,
  type SessionSettings,
} from "./settingsState";
import {
  applyUpdate,
  canCancel,
  canSend,
  disconnected,
  initialState,
  promptFailed,
  promptFinished,
  promptSent,
  type ChatState,
  type Entry,
  type SessionUpdate,
  type StopReason,
} from "./chatState";

/** Fails loudly at startup rather than leaving a dead button in the window. */
function element<T extends HTMLElement>(id: string): T {
  const found = document.getElementById(id);
  if (found === null) {
    throw new Error(`the window markup is missing #${id}`);
  }
  return found as T;
}

const transcriptEl = element<HTMLOListElement>("transcript");
const statusEl = element<HTMLParagraphElement>("status");
const draftEl = element<HTMLTextAreaElement>("draft");
const sendEl = element<HTMLButtonElement>("send");
const cancelEl = element<HTMLButtonElement>("cancel");
const composerEl = element<HTMLFormElement>("composer");

const codeStatusEl = element<HTMLParagraphElement>("code-status");
const codeCrumbsEl = element<HTMLElement>("code-crumbs");
const codeEntriesEl = element<HTMLUListElement>("code-entries");
const codeContentEl = element<HTMLPreElement>("code-content");
const settingsRootEl = element<HTMLParagraphElement>("settings-root");
const settingsRowsEl = element<HTMLUListElement>("settings-rows");

/** Each tab, the screen it reveals, and what opening it asks the shell for. */
const SCREENS = [
  { tab: "tab-chat", panel: "screen-chat", open: undefined },
  { tab: "tab-code", panel: "screen-code", open: () => openDirectory(code.path) },
  { tab: "tab-settings", panel: "screen-settings", open: loadSettings },
] as const;

let state: ChatState = initialState();
/** Cleared only after `start_session` succeeds, so Send cannot outrun the session. */
let ready = false;

function advance(next: ChatState): void {
  state = next;
  render();
}

/** Wording for whichever `StopReason` the agent actually returned. */
function stopLabel(stop: StopReason): string {
  switch (stop) {
    case "end_turn":
      return "Ready.";
    case "cancelled":
      return "Cancelled. The step that was already running finished.";
    case "max_tokens":
      return "Stopped: the token budget for this run was spent.";
    case "max_turn_requests":
      return "Stopped: the iteration budget for this run was spent.";
    case "refusal":
      return "Stopped without a final answer.";
    default:
      return "Ready.";
  }
}

function statusText(): string {
  if (!state.connected) {
    return state.error ?? "The agent is not running.";
  }
  if (state.error !== null) {
    return `Error: ${state.error}`;
  }
  if (state.pending) {
    return "Working… the transcript appears when the turn ends.";
  }
  if (!ready) {
    return "Starting the agent…";
  }
  return state.lastStop === null ? "Ready." : stopLabel(state.lastStop);
}

function row(entry: Entry): HTMLLIElement {
  const li = document.createElement("li");
  if (entry.kind === "tool") {
    li.className = `entry entry--tool entry--tool-${entry.status}`;
    const title = document.createElement("p");
    title.className = "entry__title";
    // `textContent`, never `innerHTML`: tool output is model- and
    // filesystem-derived text, and external content is data, never
    // instruction or markup (Constitution VI).
    title.textContent = `${entry.title} — ${entry.status}`;
    li.appendChild(title);
    if (entry.output !== "") {
      const body = document.createElement("pre");
      body.className = "entry__output";
      body.textContent = entry.output;
      li.appendChild(body);
    }
    return li;
  }

  li.className = `entry entry--${entry.kind}`;
  const body = document.createElement("p");
  body.className = "entry__body";
  body.textContent = entry.text;
  li.appendChild(body);
  return li;
}

function render(): void {
  transcriptEl.replaceChildren(...state.transcript.map(row));
  transcriptEl.scrollTop = transcriptEl.scrollHeight;
  statusEl.textContent = statusText();
  sendEl.disabled = !(ready && canSend(state, draftEl.value));
  cancelEl.disabled = !canCancel(state);
  draftEl.disabled = !state.connected;
}

async function send(): Promise<void> {
  const text = draftEl.value.trim();
  if (!ready || !canSend(state, text)) {
    return;
  }
  draftEl.value = "";
  advance(promptSent(state, text));
  try {
    const stop = await invoke<StopReason>("send_prompt", { text });
    advance(promptFinished(state, stop));
  } catch (error) {
    advance(promptFailed(state, String(error)));
  }
}

async function cancel(): Promise<void> {
  if (!canCancel(state)) {
    return;
  }
  try {
    await invoke("cancel_run");
  } catch (error) {
    advance(promptFailed(state, String(error)));
  }
}

composerEl.addEventListener("submit", (event) => {
  event.preventDefault();
  void send();
});
cancelEl.addEventListener("click", () => void cancel());
draftEl.addEventListener("input", render);
draftEl.addEventListener("keydown", (event) => {
  if (event.key === "Enter" && !event.shiftKey) {
    event.preventDefault();
    void send();
  }
});

async function start(): Promise<void> {
  // Relayed 1:1 from the ACP connection; the reducer decides what each one
  // means, this listener decides nothing.
  await listen<SessionUpdate>("session-update", (event) => {
    advance(applyUpdate(state, event.payload));
  });
  // The child process died. Without this the window would sit "Working…"
  // forever on a pipe with nobody at the other end.
  await listen<string>("agent-exited", (event) => {
    ready = false;
    advance(disconnected(state, event.payload));
  });

  try {
    await invoke<string>("start_session");
    ready = true;
    render();
  } catch (error) {
    advance(disconnected(state, `Could not start the agent: ${String(error)}`));
  }
}

// ---------------------------------------------------------------------------
// The Code screen. Two commands, both reads, both scoped to the one directory
// the operator named — the shell refuses everything else before this file ever
// sees it.
// ---------------------------------------------------------------------------

let code: CodeState = initialCodeState();

async function openDirectory(path: string): Promise<void> {
  code = loadingDirectory(code, path);
  renderCode();
  try {
    const entries = await invoke<DirEntry[]>("list_directory", { path });
    code = directoryLoaded(code, path, entries);
  } catch (error) {
    code = directoryFailed(code, path, String(error));
  }
  renderCode();
}

async function openFile(path: string): Promise<void> {
  code = fileOpening(code, path);
  renderCode();
  try {
    const content = await invoke<string>("read_file", { path });
    code = fileLoaded(code, path, content);
  } catch (error) {
    code = fileFailed(code, path, String(error));
  }
  renderCode();
}

/** A row in the tree. Directories navigate, files open. */
function entryRow(entry: DirEntry): HTMLLIElement {
  const li = document.createElement("li");
  const button = document.createElement("button");
  button.type = "button";
  const path = childPath(code.path, entry.name);
  const selected = !entry.directory && code.selected === path;
  button.className = `tree__entry ${entry.directory ? "tree__entry--dir" : "tree__entry--file"}${
    selected ? " tree__entry--selected" : ""
  }`;
  // `textContent`, never `innerHTML`: a file name comes off the operator's
  // disk, and external content is data, never markup (Constitution VI).
  button.textContent = entry.directory ? `${entry.name}/` : entry.name;
  button.addEventListener("click", () => {
    void (entry.directory ? openDirectory(path) : openFile(path));
  });
  li.appendChild(button);
  return li;
}

function upRow(parent: string): HTMLLIElement {
  const li = document.createElement("li");
  const button = document.createElement("button");
  button.type = "button";
  button.className = "tree__entry tree__entry--dir";
  button.textContent = "../";
  button.addEventListener("click", () => void openDirectory(parent));
  li.appendChild(button);
  return li;
}

function codeStatusText(): string {
  if (code.error !== null) {
    return code.error;
  }
  if (code.loading) {
    return "Reading…";
  }
  if (code.selected !== null) {
    return code.selected;
  }
  return "The files this session's agent can reach, and nothing outside them.";
}

function renderCode(): void {
  codeCrumbsEl.replaceChildren(
    ...breadcrumbs(code.path).map((crumb, index, all) => {
      const button = document.createElement("button");
      button.type = "button";
      const current = index === all.length - 1;
      button.className = `crumb${current ? " crumb--current" : ""}`;
      button.textContent = crumb.label;
      button.disabled = current;
      button.addEventListener("click", () => void openDirectory(crumb.path));
      return button;
    }),
  );

  const parent = parentOf(code.path);
  const rows = code.entries.map(entryRow);
  if (parent !== null) {
    rows.unshift(upRow(parent));
  }
  if (rows.length === 0) {
    const empty = document.createElement("li");
    empty.className = "tree__empty";
    // Said, rather than left as a blank box that could equally mean "not
    // loaded yet".
    empty.textContent = code.error === null ? "This directory is empty." : "";
    rows.push(empty);
  }
  codeEntriesEl.replaceChildren(...rows);

  codeContentEl.textContent =
    code.content ??
    (code.selected === null ? "Select a file to read it." : "");
  codeStatusEl.textContent = codeStatusText();
}

// ---------------------------------------------------------------------------
// The Settings screen. One command, and nothing to click: every row reports a
// flag the child was launched with, which only a restart can change.
// ---------------------------------------------------------------------------

function settingsRow(row: ReturnType<typeof settingsRows>[number]): HTMLLIElement {
  const li = document.createElement("li");
  li.className = `settings__row settings__row--${row.state}`;

  const name = document.createElement("p");
  name.className = "settings__name";
  name.textContent = row.label;
  const state = document.createElement("span");
  state.className = "settings__state";
  state.textContent = ` — ${row.status}`;
  name.appendChild(state);
  li.appendChild(name);

  if (row.tools !== "") {
    const tools = document.createElement("p");
    tools.className = "settings__tools";
    tools.textContent = row.tools;
    li.appendChild(tools);
  }

  const detail = document.createElement("p");
  detail.className = "settings__detail";
  detail.textContent = row.detail;
  li.appendChild(detail);
  return li;
}

async function loadSettings(): Promise<void> {
  try {
    const settings = await invoke<SessionSettings>("session_settings");
    settingsRootEl.textContent = `Root: ${rootLabel(settings)}`;
    settingsRowsEl.replaceChildren(...settingsRows(settings).map(settingsRow));
  } catch (error) {
    settingsRootEl.textContent = String(error);
    settingsRowsEl.replaceChildren();
  }
}

// ---------------------------------------------------------------------------
// The tab strip. Re-reads on every open rather than caching: a screen that
// silently shows a stale answer is worse than one that costs a `read_dir`.
// ---------------------------------------------------------------------------

function showScreen(tab: string): void {
  for (const screen of SCREENS) {
    const active = screen.tab === tab;
    const tabEl = element<HTMLButtonElement>(screen.tab);
    tabEl.classList.toggle("tab--active", active);
    tabEl.setAttribute("aria-selected", String(active));
    element<HTMLElement>(screen.panel).hidden = !active;
    if (active && screen.open !== undefined) {
      void screen.open();
    }
  }
}

for (const screen of SCREENS) {
  element<HTMLButtonElement>(screen.tab).addEventListener("click", () =>
    showScreen(screen.tab),
  );
}

showScreen("tab-chat");
render();
renderCode();
void start();
