// Unit tests for the one piece of frontend logic that is not DOM glue.
//
// Written before `chatState.ts` exists (Constitution III). Every fixture below
// is the **wire** shape `skein acp-agent` actually puts on the ACP connection —
// `SessionUpdate` is `#[serde(tag = "sessionUpdate", rename_all = "snake_case")]`
// in `agent-client-protocol`, and `ToolCall`/`ToolCallUpdate` are `camelCase` —
// so a change to the protocol breaks these tests rather than the running app.

import { describe, expect, it } from "vitest";
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
  type SessionUpdate,
} from "./chatState";

const text = (t: string): SessionUpdate => ({
  sessionUpdate: "agent_message_chunk",
  content: { type: "text", text: t },
});

const toolCall = (id: string, title: string): SessionUpdate => ({
  sessionUpdate: "tool_call",
  toolCallId: id,
  title,
  kind: "other",
});

const toolDone = (id: string, output: string): SessionUpdate => ({
  sessionUpdate: "tool_call_update",
  toolCallId: id,
  status: "completed",
  content: [{ type: "content", content: { type: "text", text: output } }],
});

const toolFailed = (id: string): SessionUpdate => ({
  sessionUpdate: "tool_call_update",
  toolCallId: id,
  status: "failed",
});

/** Replays a whole turn's worth of updates, which is how they really arrive. */
const replay = (state: ChatState, updates: SessionUpdate[]): ChatState =>
  updates.reduce(applyUpdate, state);

describe("initialState", () => {
  it("starts empty, connected, and with nothing in flight", () => {
    const state = initialState();
    expect(state.transcript).toEqual([]);
    expect(state.pending).toBe(false);
    expect(state.connected).toBe(true);
    expect(state.lastStop).toBeNull();
    expect(state.error).toBeNull();
  });
});

describe("applyUpdate - agent_message_chunk", () => {
  it("appends the assistant's text to the transcript", () => {
    const state = applyUpdate(initialState(), text("the answer is 42"));
    expect(state.transcript).toEqual([
      { kind: "assistant", text: "the answer is 42" },
    ]);
  });

  it("keeps one entry per chunk rather than merging them", () => {
    // `skein-acp` emits exactly one chunk per `LlmResponse` step, so two
    // entries here means two model turns - merging would hide that.
    const state = replay(initialState(), [text("first"), text("second")]);
    expect(state.transcript.map((e) => e.kind)).toEqual([
      "assistant",
      "assistant",
    ]);
  });

  it("ignores a non-text content block instead of rendering [object Object]", () => {
    const state = applyUpdate(initialState(), {
      sessionUpdate: "agent_message_chunk",
      content: { type: "image", data: "...", mimeType: "image/png" },
    } as unknown as SessionUpdate);
    expect(state.transcript).toEqual([]);
  });
});

describe("applyUpdate - tool calls", () => {
  it("adds a pending card for a tool_call", () => {
    const state = applyUpdate(initialState(), toolCall("step-7", "fs_write"));
    expect(state.transcript).toEqual([
      {
        kind: "tool",
        id: "step-7",
        title: "fs_write",
        status: "pending",
        output: "",
      },
    ]);
  });

  it("completes the matching card and shows its output", () => {
    const state = replay(initialState(), [
      toolCall("step-7", "fs_write"),
      toolDone("step-7", "wrote 12 bytes"),
    ]);
    expect(state.transcript).toEqual([
      {
        kind: "tool",
        id: "step-7",
        title: "fs_write",
        status: "completed",
        output: "wrote 12 bytes",
      },
    ]);
  });

  it("marks a rejected tool call failed without inventing output", () => {
    const state = replay(initialState(), [
      toolCall("step-7", "fs_write"),
      toolFailed("step-7"),
    ]);
    expect(state.transcript[0]).toMatchObject({ status: "failed", output: "" });
  });

  it("updates the right card when two tool calls are open", () => {
    const state = replay(initialState(), [
      toolCall("step-1", "fs_read"),
      toolCall("step-4", "fs_write"),
      toolDone("step-4", "ok"),
    ]);
    expect(state.transcript).toMatchObject([
      { id: "step-1", status: "pending" },
      { id: "step-4", status: "completed", output: "ok" },
    ]);
  });

  it("ignores a tool_call_update for a card it never saw", () => {
    const state = applyUpdate(initialState(), toolDone("ghost", "ok"));
    expect(state.transcript).toEqual([]);
  });
});

describe("applyUpdate - forward compatibility", () => {
  it("ignores a variant this UI does not render", () => {
    // `SessionUpdate` is `#[non_exhaustive]`; an unknown discriminator must not
    // throw in a desktop app whose only recovery is a restart.
    const state = applyUpdate(initialState(), {
      sessionUpdate: "usage_update",
    } as unknown as SessionUpdate);
    expect(state).toEqual(initialState());
  });
});

describe("the batch a single turn delivers", () => {
  it("keeps pending set until the prompt itself resolves", () => {
    // Correction 3: `skein-acp` sends every update for a run in one burst
    // *before* it answers `session/prompt`. Clearing `pending` on the first
    // update would re-enable Send while the turn is still running.
    let state = promptSent(initialState(), "do the thing");
    state = replay(state, [
      text("I will write a file"),
      toolCall("step-3", "fs_write"),
      toolDone("step-3", "wrote 12 bytes"),
      text("done"),
    ]);
    expect(state.pending).toBe(true);
    expect(canSend(state)).toBe(false);

    state = promptFinished(state, "end_turn");
    expect(state.pending).toBe(false);
    expect(state.lastStop).toBe("end_turn");
    expect(canSend(state)).toBe(true);
  });
});

describe("promptSent", () => {
  it("records the user's message and blocks a second send", () => {
    const state = promptSent(initialState(), "hello");
    expect(state.transcript).toEqual([{ kind: "user", text: "hello" }]);
    expect(state.pending).toBe(true);
    expect(canSend(state)).toBe(false);
    expect(canCancel(state)).toBe(true);
  });

  it("clears a previous turn's error and stop reason", () => {
    const failed = promptFailed(promptSent(initialState(), "one"), "boom");
    const state = promptSent(failed, "two");
    expect(state.error).toBeNull();
    expect(state.lastStop).toBeNull();
  });
});

describe("stop reasons", () => {
  it("reports a cancelled turn as cancelled, not as a completed one", () => {
    const state = promptFinished(promptSent(initialState(), "hi"), "cancelled");
    expect(state.lastStop).toBe("cancelled");
    expect(state.error).toBeNull();
    expect(state.pending).toBe(false);
  });

  it("distinguishes end_turn from cancelled", () => {
    const state = promptFinished(promptSent(initialState(), "hi"), "end_turn");
    expect(state.lastStop).toBe("end_turn");
  });
});

describe("failure and disconnection", () => {
  it("surfaces a prompt failure and lets the user try again", () => {
    const state = promptFailed(
      promptSent(initialState(), "hi"),
      "acp: broken pipe",
    );
    expect(state.error).toBe("acp: broken pipe");
    expect(state.pending).toBe(false);
    expect(canSend(state)).toBe(true);
  });

  it("locks the UI when the agent process is gone, instead of hanging", () => {
    const state = disconnected(
      promptSent(initialState(), "hi"),
      "the agent exited",
    );
    expect(state.connected).toBe(false);
    expect(state.pending).toBe(false);
    expect(state.error).toBe("the agent exited");
    expect(canSend(state)).toBe(false);
    expect(canCancel(state)).toBe(false);
  });
});

describe("send and cancel guards", () => {
  it("refuses an empty or whitespace-only message", () => {
    // Mirrors `crates/skein-cli/src/chat.rs`'s refusal of an empty prompt: the
    // guard exists in the core, and the UI must not be the first to relax it.
    expect(canSend(initialState(), "")).toBe(false);
    expect(canSend(initialState(), "   \n ")).toBe(false);
    expect(canSend(initialState(), " hi ")).toBe(true);
  });

  it("makes cancel a no-op when no prompt is in flight", () => {
    expect(canCancel(initialState())).toBe(false);
  });
});

describe("immutability", () => {
  it("never mutates the state it was handed", () => {
    const before = promptSent(initialState(), "hi");
    const snapshot = JSON.stringify(before);
    applyUpdate(before, text("hello"));
    applyUpdate(before, toolCall("step-1", "fs_read"));
    promptFinished(before, "end_turn");
    expect(JSON.stringify(before)).toBe(snapshot);
  });
});
