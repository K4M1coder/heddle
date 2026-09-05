// Unit tests for the Settings screen's one mapping.
//
// Written before `settingsState.ts` exists (Constitution III). Every fixture is
// the **wire** shape `session_settings` really returns — `settings.rs` is
// `#[serde(rename_all = "camelCase")]`, so `fsRoot` and `notWiredToSession` are
// the field and variant names that cross the boundary, and a rename on the Rust
// side breaks a test here rather than the running window.

import { describe, expect, it } from "vitest";
import {
  rootLabel,
  settingsRows,
  type ConnectorStatus,
  type SessionSettings,
} from "./settingsState";

const connector = (
  name: string,
  state: ConnectorStatus["state"],
  tools: string[],
  detail = "because.",
): ConnectorStatus => ({ name, state, tools, detail });

const settings = (
  fsRoot: string | null,
  connectors: ConnectorStatus[],
): SessionSettings => ({ fsRoot, connectors });

describe("settingsRows", () => {
  it("labels each connector for a reader rather than showing its identifier", () => {
    const rows = settingsRows(
      settings("C:/work", [
        connector("fs", "enabled", ["fs_read"]),
        connector("git", "disabled", []),
        connector("shell", "disabled", []),
        connector("atlassian", "notWiredToSession", []),
        connector("m365", "notWiredToSession", []),
      ]),
    );
    expect(rows.map((row) => row.label)).toEqual([
      "Filesystem",
      "Git",
      "Shell",
      "Atlassian",
      "Microsoft 365",
    ]);
  });

  it("says which of the three states each connector is in, in words", () => {
    const rows = settingsRows(
      settings("C:/work", [
        connector("fs", "enabled", ["fs_read", "fs_list"]),
        connector("git", "disabled", []),
        connector("atlassian", "notWiredToSession", []),
      ]),
    );
    expect(rows.map((row) => row.status)).toEqual([
      "Enabled",
      "Disabled",
      "Not wired to a session",
    ]);
  });

  it("keeps the connectors in the order the command returned them", () => {
    const rows = settingsRows(
      settings(null, [
        connector("shell", "disabled", []),
        connector("fs", "disabled", []),
      ]),
    );
    expect(rows.map((row) => row.name)).toEqual(["shell", "fs"]);
  });

  it("relays the reason verbatim, because the reason is the Rust side's fact", () => {
    const detail =
      "The Atlassian connector is compiled in, but no flag on any `heddle` subcommand wires it to a session yet.";
    const [row] = settingsRows(
      settings(null, [connector("atlassian", "notWiredToSession", [], detail)]),
    );
    expect(row?.detail).toBe(detail);
  });

  it("names the tools an enabled connector really offers, and none otherwise", () => {
    const rows = settingsRows(
      settings("C:/work", [
        connector("fs", "enabled", ["fs_read", "fs_list", "fs_write"]),
        connector("git", "disabled", []),
      ]),
    );
    expect(rows[0]?.tools).toBe("fs_read, fs_list, fs_write");
    expect(rows[1]?.tools).toBe("");
  });

  it("labels a connector name it has never heard of instead of dropping the row", () => {
    // `settings.rs` is where connectors are enumerated. A future one must show
    // up here as an unstyled row, never as a silently missing capability.
    const [row] = settingsRows(
      settings(null, [connector("slack", "notWiredToSession", [])]),
    );
    expect(row?.label).toBe("slack");
    expect(row?.status).toBe("Not wired to a session");
  });
});

describe("rootLabel", () => {
  it("shows the directory the operator named", () => {
    expect(rootLabel(settings("C:/work", []))).toBe("C:/work");
  });

  it("says there is no root rather than showing an empty line", () => {
    // Absent a root the session has no tools at all, and a blank field would
    // read as a missing value rather than as the deliberate default it is.
    expect(rootLabel(settings(null, []))).toBe(
      "not set — this session has no tools",
    );
  });
});
