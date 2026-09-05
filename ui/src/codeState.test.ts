// Unit tests for the Code screen's view state.
//
// Written before `codeState.ts` exists (Constitution III). Every fixture is the
// **wire** shape `list_directory` really returns — `code::Entry` serialises as
// `{ name, directory }` — so a change to the Rust struct breaks a test here
// rather than the running window.

import { describe, expect, it } from "vitest";
import {
  breadcrumbs,
  childPath,
  directoryFailed,
  directoryLoaded,
  fileFailed,
  fileLoaded,
  fileOpening,
  initialState,
  loadingDirectory,
  parentOf,
  type CodeState,
  type DirEntry,
} from "./codeState";

const dir = (name: string): DirEntry => ({ name, directory: true });
const file = (name: string): DirEntry => ({ name, directory: false });

/** The root, listed once, which is what the window does when Code opens. */
const opened = (): CodeState =>
  directoryLoaded(loadingDirectory(initialState(), "."), ".", [
    dir("src"),
    file("main.rs"),
  ]);

describe("initialState", () => {
  it("shows nothing, has nothing selected, and is not mid-read", () => {
    const state = initialState();
    expect(state.path).toBe(".");
    expect(state.entries).toEqual([]);
    expect(state.selected).toBeNull();
    expect(state.content).toBeNull();
    expect(state.loading).toBe(false);
    expect(state.error).toBeNull();
  });
});

describe("directoryLoaded", () => {
  it("shows the entries the command really returned, in the order it returned them", () => {
    // The order is the Rust side's: directories first, then names. The reducer
    // must not re-sort, or a redraw could disagree with what `fs_list` shows
    // the agent for the same directory.
    const state = opened();
    expect(state.path).toBe(".");
    expect(state.entries).toEqual([dir("src"), file("main.rs")]);
    expect(state.loading).toBe(false);
    expect(state.error).toBeNull();
  });

  it("drops a listing that is not for the directory being opened", () => {
    // Two clicks in flight: the slower answer must not repaint the tree the
    // operator is already looking at.
    const state = loadingDirectory(opened(), "src");
    const stale = directoryLoaded(state, "docs", [file("README.md")]);
    expect(stale.path).toBe(".");
    expect(stale.entries).toEqual([dir("src"), file("main.rs")]);
    expect(stale.loading).toBe(true);
  });

  it("clears the open file when the operator navigates away", () => {
    const reading = fileLoaded(
      fileOpening(opened(), "main.rs"),
      "main.rs",
      "fn main() {}",
    );
    const moved = directoryLoaded(loadingDirectory(reading, "src"), "src", [
      file("lib.rs"),
    ]);
    expect(moved.selected).toBeNull();
    expect(moved.content).toBeNull();
  });
});

describe("directoryFailed", () => {
  it("shows the reason the command gave and leaves the last good listing up", () => {
    const state = directoryFailed(
      loadingDirectory(opened(), "src"),
      "src",
      "src: permission denied",
    );
    expect(state.error).toBe("src: permission denied");
    expect(state.loading).toBe(false);
    expect(state.path).toBe(".");
    expect(state.entries).toEqual([dir("src"), file("main.rs")]);
  });

  it("relays the no-fs-root refusal verbatim rather than restating it", () => {
    // The sentence belongs to `code.rs`, which is where the fact lives. A
    // second copy here could drift from the one the Rust side pins.
    const refusal =
      "this session has no fs-root: there is nothing to browse, and the agent has no tools either";
    const state = directoryFailed(
      loadingDirectory(initialState(), "."),
      ".",
      refusal,
    );
    expect(state.error).toBe(refusal);
    expect(state.entries).toEqual([]);
  });
});

describe("fileOpening", () => {
  it("selects the file and goes to work, clearing the previous file's text", () => {
    const state = fileOpening(
      fileLoaded(fileOpening(opened(), "main.rs"), "main.rs", "fn main() {}"),
      "other.rs",
    );
    expect(state.selected).toBe("other.rs");
    expect(state.loading).toBe(true);
    expect(state.content).toBeNull();
    expect(state.error).toBeNull();
  });
});

describe("fileLoaded", () => {
  it("shows the file's real content", () => {
    const state = fileLoaded(
      fileOpening(opened(), "main.rs"),
      "main.rs",
      "fn main() {}\n",
    );
    expect(state.content).toBe("fn main() {}\n");
    expect(state.selected).toBe("main.rs");
    expect(state.loading).toBe(false);
  });

  it("keeps an empty file empty rather than treating it as nothing read", () => {
    const state = fileLoaded(fileOpening(opened(), "empty.txt"), "empty.txt", "");
    expect(state.content).toBe("");
    expect(state.loading).toBe(false);
  });

  it("drops an answer for a file that is no longer selected", () => {
    const state = fileOpening(fileOpening(opened(), "first.rs"), "second.rs");
    const stale = fileLoaded(state, "first.rs", "the wrong file");
    expect(stale.content).toBeNull();
    expect(stale.selected).toBe("second.rs");
    expect(stale.loading).toBe(true);
  });
});

describe("fileFailed", () => {
  it("shows the refusal instead of any content at all", () => {
    // Not "the previous file's text with an error beside it": a pane showing
    // one file's bytes under another file's name is the window lying about
    // what is on disk, which is the one thing this screen must not do.
    const state = fileFailed(
      fileOpening(
        fileLoaded(fileOpening(opened(), "main.rs"), "main.rs", "fn main() {}"),
        "icon.bin",
      ),
      "icon.bin",
      "icon.bin is not UTF-8 text and cannot be shown; this view reads text files only",
    );
    expect(state.content).toBeNull();
    expect(state.error).toContain("not UTF-8 text");
    expect(state.selected).toBe("icon.bin");
    expect(state.loading).toBe(false);
    // The tree is untouched: a file that cannot be read is not a broken
    // directory.
    expect(state.entries).toEqual([dir("src"), file("main.rs")]);
  });

  it("drops a failure for a file that is no longer selected", () => {
    const state = fileOpening(fileOpening(opened(), "first.rs"), "second.rs");
    expect(fileFailed(state, "first.rs", "gone").error).toBeNull();
  });
});

describe("path helpers", () => {
  it("joins a name onto the directory it was listed in", () => {
    expect(childPath(".", "src")).toBe("src");
    expect(childPath("src", "lib.rs")).toBe("src/lib.rs");
    expect(childPath("a/b", "c.rs")).toBe("a/b/c.rs");
  });

  it("walks back up, and stops at the root", () => {
    expect(parentOf("a/b/c.rs")).toBe("a/b");
    expect(parentOf("src")).toBe(".");
    expect(parentOf(".")).toBeNull();
  });

  it("names every step of the way back, root first", () => {
    expect(breadcrumbs(".")).toEqual([{ label: "root", path: "." }]);
    expect(breadcrumbs("a/b")).toEqual([
      { label: "root", path: "." },
      { label: "a", path: "a" },
      { label: "b", path: "a/b" },
    ]);
  });
});
