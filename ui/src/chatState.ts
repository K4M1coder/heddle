/**
 * The Chat screen's view state, and the pure functions that advance it.
 *
 * This module owns **all** of the frontend's logic and none of its DOM:
 * `main.ts` is glue that calls these functions and paints the result. That
 * split is Constitution I's "the UI adds no capability of its own" made
 * checkable — everything here is a projection of what `skein acp-agent` already
 * sent, and everything testable lives in one file with no Tauri import in it.
 *
 * The `SessionUpdate` types below are the **wire** shapes, not a convenient
 * re-modelling of them: `agent-client-protocol`'s `SessionUpdate` is an
 * externally-tagged-by-`sessionUpdate` enum whose payload fields are
 * `camelCase`. Matching the wire means a protocol change surfaces as a failing
 * test in `chatState.test.ts` instead of as a blank transcript at runtime.
 */

/**
 * `ContentBlock`, of which this UI renders exactly the text variant.
 *
 * Modelled as one open shape rather than a discriminated union on purpose: the
 * Rust enum is `#[non_exhaustive]`, so a closed union here would be a claim
 * about the protocol that the protocol does not make.
 */
export type ContentBlock = { type: string; text?: string };

/** `ToolCallContent::Content`, the only variant `project_updates` produces. */
export type ToolCallContent = { type: string; content?: ContentBlock };

/** `agent_client_protocol::schema::v1::ToolCallStatus`. */
export type ToolCallStatus = "pending" | "in_progress" | "completed" | "failed";

/** `agent_client_protocol::schema::v1::StopReason`. */
export type StopReason =
  | "end_turn"
  | "max_tokens"
  | "max_turn_requests"
  | "refusal"
  | "cancelled";

/**
 * The `SessionUpdate` variants this screen renders. `SessionUpdate` is
 * `#[non_exhaustive]` on the Rust side, so the catch-all arm is part of the
 * contract rather than defensive padding.
 */
export type SessionUpdate =
  | { sessionUpdate: "agent_message_chunk"; content: ContentBlock }
  | {
      sessionUpdate: "tool_call";
      toolCallId: string;
      title: string;
      kind?: string;
      status?: ToolCallStatus;
    }
  | {
      sessionUpdate: "tool_call_update";
      toolCallId: string;
      status?: ToolCallStatus;
      title?: string;
      content?: ToolCallContent[];
    }
  | { sessionUpdate: string };

/** One rendered row of the transcript. */
export type Entry =
  | { kind: "user"; text: string }
  | { kind: "assistant"; text: string }
  | {
      kind: "tool";
      id: string;
      title: string;
      status: ToolCallStatus;
      output: string;
    };

export interface ChatState {
  /** Every row to paint, in the order the chain produced it. */
  readonly transcript: readonly Entry[];
  /** A `session/prompt` is in flight: Send is closed, Cancel is open. */
  readonly pending: boolean;
  /** The last turn's `StopReason`, so a cancelled turn does not look normal. */
  readonly lastStop: StopReason | null;
  /** A transport- or agent-level failure, shown once and cleared on the next send. */
  readonly error: string | null;
  /** False once the `skein acp-agent` child is gone; nothing can be sent after that. */
  readonly connected: boolean;
}

export function initialState(): ChatState {
  return {
    transcript: [],
    pending: false,
    lastStop: null,
    error: null,
    connected: true,
  };
}

/** The text of a content block, or null for a shape this UI does not render. */
function plainText(block: ContentBlock | undefined): string | null {
  if (block === undefined || block.type !== "text") {
    return null;
  }
  return typeof block.text === "string" ? block.text : null;
}

/** The first textual payload of a tool result, flattened for a card body. */
function toolOutput(content: ToolCallContent[] | undefined): string | null {
  if (content === undefined) {
    return null;
  }
  const texts = content
    .map((item) => (item.type === "content" ? plainText(item.content) : null))
    .filter((text): text is string => text !== null);
  return texts.length > 0 ? texts.join("\n") : null;
}

/**
 * Folds one ACP `session/update` notification into the view state.
 *
 * Deliberately does **not** touch `pending`. `skein-acp` sends a run's whole
 * batch of updates before it answers `session/prompt`
 * (`crates/skein-acp/src/lib.rs`), so an update is never evidence that the turn
 * is over — only `promptFinished` is.
 */
export function applyUpdate(state: ChatState, update: SessionUpdate): ChatState {
  switch (update.sessionUpdate) {
    case "agent_message_chunk": {
      const text = plainText((update as { content: ContentBlock }).content);
      if (text === null) {
        return state;
      }
      return { ...state, transcript: [...state.transcript, { kind: "assistant", text }] };
    }

    case "tool_call": {
      const call = update as { toolCallId: string; title: string; status?: ToolCallStatus };
      return {
        ...state,
        transcript: [
          ...state.transcript,
          {
            kind: "tool",
            id: call.toolCallId,
            title: call.title,
            status: call.status ?? "pending",
            output: "",
          },
        ],
      };
    }

    case "tool_call_update": {
      const change = update as {
        toolCallId: string;
        status?: ToolCallStatus;
        title?: string;
        content?: ToolCallContent[];
      };
      // An update for a card that was never announced is dropped rather than
      // turned into a card of its own: the id is the chain step id, so a card
      // with no `tool_call` before it would be a row with no provenance.
      let matched = false;
      const transcript = state.transcript.map((entry) => {
        if (entry.kind !== "tool" || entry.id !== change.toolCallId) {
          return entry;
        }
        matched = true;
        const output = toolOutput(change.content);
        return {
          ...entry,
          title: change.title ?? entry.title,
          status: change.status ?? entry.status,
          output: output ?? entry.output,
        };
      });
      return matched ? { ...state, transcript } : state;
    }

    default:
      return state;
  }
}

/** The user pressed Send: the message is on the transcript and the turn is open. */
export function promptSent(state: ChatState, text: string): ChatState {
  return {
    ...state,
    transcript: [...state.transcript, { kind: "user", text }],
    pending: true,
    lastStop: null,
    error: null,
  };
}

/** `session/prompt` answered. The `StopReason` is shown as-is, success or not. */
export function promptFinished(state: ChatState, stop: StopReason): ChatState {
  return { ...state, pending: false, lastStop: stop };
}

/** The prompt failed rather than stopped: the session survives, the turn did not. */
export function promptFailed(state: ChatState, message: string): ChatState {
  return { ...state, pending: false, error: message };
}

/**
 * The `skein acp-agent` child is gone. Everything closes: a desktop app whose
 * agent died must say so, not keep accepting messages into a dead pipe.
 */
export function disconnected(state: ChatState, message: string): ChatState {
  return { ...state, connected: false, pending: false, error: message };
}

/**
 * Whether Send may fire. With a `draft`, also whether that draft is worth
 * sending — `skein chat` refuses an empty prompt, and the UI must not be the
 * first layer to relax a core rule.
 */
export function canSend(state: ChatState, draft?: string): boolean {
  if (!state.connected || state.pending) {
    return false;
  }
  return draft === undefined || draft.trim().length > 0;
}

/** Whether Cancel may fire. With no turn in flight there is nothing to cancel. */
export function canCancel(state: ChatState): boolean {
  return state.connected && state.pending;
}
