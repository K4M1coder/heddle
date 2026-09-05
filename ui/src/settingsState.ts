/**
 * The Settings screen's one mapping: the connector status the Rust shell
 * reported, turned into rows a reader can act on.
 *
 * `chatState.ts`'s pattern — pure functions over the wire shape, no DOM, no
 * Tauri import, unit-tested in `node`. The only thing added on the way through
 * is **wording**: a label and a status phrase for a human. Every fact stays the
 * Rust side's, and `detail` is relayed verbatim rather than rewritten, because
 * the reason a connector is off is a property of the session and not of the
 * screen showing it.
 *
 * There is deliberately **no toggle**, here or anywhere below this file.
 * Turning a connector on is `heddle acp-agent`'s flags; a switch in this window
 * would be the UI serving a capability the CLI does not (Constitution I).
 */

/** `settings::ConnectorState`, `#[serde(rename_all = "camelCase")]`. */
export type ConnectorState = "enabled" | "disabled" | "notWiredToSession";

/** `settings::ConnectorStatus`. */
export interface ConnectorStatus {
  readonly name: string;
  readonly state: ConnectorState;
  readonly tools: readonly string[];
  readonly detail: string;
}

/** `settings::SessionSettings`. */
export interface SessionSettings {
  readonly fsRoot: string | null;
  readonly connectors: readonly ConnectorStatus[];
}

/** One painted row. */
export interface SettingsRow {
  /** The connector's own name, kept for styling and for the DOM. */
  readonly name: string;
  readonly label: string;
  readonly state: ConnectorState;
  /** The state, said in words. */
  readonly status: string;
  /** The tools this session really offers for it, or `""`. */
  readonly tools: string;
  /** Why it is in that state, in the Rust shell's own words. */
  readonly detail: string;
}

/**
 * Display names for the connectors `settings.rs` enumerates today.
 *
 * A name that is not here falls through to itself rather than being dropped:
 * this file must never be the reason a capability the session really has goes
 * unmentioned on the screen that claims to list them all.
 */
const LABELS: Readonly<Record<string, string>> = {
  fs: "Filesystem",
  git: "Git",
  shell: "Shell",
  atlassian: "Atlassian",
  m365: "Microsoft 365",
};

function statusWords(state: ConnectorState): string {
  switch (state) {
    case "enabled":
      return "Enabled";
    case "disabled":
      return "Disabled";
    // Neither on nor off, and said as such: no flag reaches this connector, so
    // "Disabled" would imply an operator could flip something that does not
    // exist.
    case "notWiredToSession":
      return "Not wired to a session";
    default:
      return state;
  }
}

export function settingsRows(settings: SessionSettings): SettingsRow[] {
  return settings.connectors.map((connector) => ({
    name: connector.name,
    label: LABELS[connector.name] ?? connector.name,
    state: connector.state,
    status: statusWords(connector.state),
    tools: connector.tools.join(", "),
    detail: connector.detail,
  }));
}

/** The session's root, or the statement that it has none. */
export function rootLabel(settings: SessionSettings): string {
  return settings.fsRoot ?? "not set — this session has no tools";
}
