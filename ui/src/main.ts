/**
 * DOM glue for the Chat screen, and nothing else.
 *
 * Every decision this file could make has been moved into `chatState.ts` so it
 * can be unit-tested without a browser: this module reads the state, paints it,
 * and forwards three user actions to the three Tauri commands that are, in
 * turn, three ACP calls the CLI already serves (`session/new`,
 * `session/prompt`, `session/cancel`). `docs/UI.md` holds the full mapping.
 */

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
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
    throw new Error(`the chat markup is missing #${id}`);
  }
  return found as T;
}

const transcriptEl = element<HTMLOListElement>("transcript");
const statusEl = element<HTMLParagraphElement>("status");
const draftEl = element<HTMLTextAreaElement>("draft");
const sendEl = element<HTMLButtonElement>("send");
const cancelEl = element<HTMLButtonElement>("cancel");
const composerEl = element<HTMLFormElement>("composer");

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

render();
void start();
