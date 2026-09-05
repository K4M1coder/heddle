/**
 * The Code screen's view state, and the pure functions that advance it.
 *
 * `chatState.ts`'s split, one screen over: this module owns every decision the
 * Code view makes and none of its DOM, so it is unit-tested in `node` with no
 * browser and no Tauri import. `main.ts` calls `list_directory` / `read_file`
 * and folds the answers through here.
 *
 * **Nothing here decides what may be read.** Containment is
 * `heddle_connectors::FsRoot`'s, in the Rust shell, and every refusal below is
 * relayed verbatim rather than restated: a second copy of "this path left the
 * root" in TypeScript could drift from the one the product actually enforces.
 *
 * Paths are always relative to the session's `--fs-root`, with `"."` for the
 * root itself, separated by `/`. That is what `FsRoot` accepts on every
 * platform — Windows resolves `a/b` the same as `a\b` — so the frontend never
 * has to know which OS it is painting on.
 */

/** One row of a listing, as `code::Entry` serialises. */
export interface DirEntry {
  readonly name: string;
  readonly directory: boolean;
}

/** One step of the path back to the root. */
export interface Crumb {
  readonly label: string;
  readonly path: string;
}

export interface CodeState {
  /** The directory currently shown. `"."` is the fs-root itself. */
  readonly path: string;
  /**
   * Its children, in the order the Rust side returned them — directories
   * first, then names. Deliberately not re-sorted here: a second ordering
   * would let the window disagree with what `fs_list` shows the agent for the
   * same directory.
   */
  readonly entries: readonly DirEntry[];
  /** The file whose content the pane is showing or fetching, if any. */
  readonly selected: string | null;
  /** That file's real content. `""` is an empty file, not "nothing read". */
  readonly content: string | null;
  /** A `list_directory` or `read_file` is in flight. */
  readonly loading: boolean;
  /** The last refusal, in the Rust shell's own words. */
  readonly error: string | null;
  /**
   * What is being fetched, so a slow answer for an abandoned click can be
   * dropped instead of repainting the screen the operator moved on from.
   */
  readonly awaiting: string | null;
}

export function initialState(): CodeState {
  return {
    path: ".",
    entries: [],
    selected: null,
    content: null,
    loading: false,
    error: null,
    awaiting: null,
  };
}

/** A directory was asked for. Nothing is repainted until the answer arrives. */
export function loadingDirectory(state: CodeState, path: string): CodeState {
  return { ...state, loading: true, error: null, awaiting: `dir:${path}` };
}

/**
 * A listing came back. Navigating closes the open file: a content pane holding
 * one directory's file while the tree shows another is a screen that has to be
 * read twice to be believed.
 */
export function directoryLoaded(
  state: CodeState,
  path: string,
  entries: readonly DirEntry[],
): CodeState {
  if (state.awaiting !== `dir:${path}`) {
    return state;
  }
  return {
    ...state,
    path,
    entries,
    selected: null,
    content: null,
    loading: false,
    error: null,
    awaiting: null,
  };
}

/**
 * A listing was refused. The last good listing stays up — a directory that
 * cannot be opened says nothing about the one already on screen.
 */
export function directoryFailed(
  state: CodeState,
  path: string,
  message: string,
): CodeState {
  if (state.awaiting !== `dir:${path}`) {
    return state;
  }
  return { ...state, loading: false, error: message, awaiting: null };
}

/** A file was clicked. The previous file's text goes immediately. */
export function fileOpening(state: CodeState, path: string): CodeState {
  return {
    ...state,
    selected: path,
    content: null,
    loading: true,
    error: null,
    awaiting: `file:${path}`,
  };
}

/** The file's real content, exactly as the shell read it off disk. */
export function fileLoaded(
  state: CodeState,
  path: string,
  content: string,
): CodeState {
  if (state.awaiting !== `file:${path}`) {
    return state;
  }
  return { ...state, content, loading: false, error: null, awaiting: null };
}

/**
 * The file could not be shown, and the pane says why instead of showing
 * anything. Never the previous file's text under this file's name: that would
 * be the window claiming something about a file it did not read.
 */
export function fileFailed(
  state: CodeState,
  path: string,
  message: string,
): CodeState {
  if (state.awaiting !== `file:${path}`) {
    return state;
  }
  return {
    ...state,
    content: null,
    loading: false,
    error: message,
    awaiting: null,
  };
}

/** The path of `name` as listed inside `directory`. */
export function childPath(directory: string, name: string): string {
  return directory === "." ? name : `${directory}/${name}`;
}

/** The directory containing `path`, or `null` when `path` is already the root. */
export function parentOf(path: string): string | null {
  if (path === ".") {
    return null;
  }
  const cut = path.lastIndexOf("/");
  return cut === -1 ? "." : path.slice(0, cut);
}

/** Every step from the root down to `path`, root first. */
export function breadcrumbs(path: string): Crumb[] {
  const crumbs: Crumb[] = [{ label: "root", path: "." }];
  if (path === ".") {
    return crumbs;
  }
  let walked = "";
  for (const part of path.split("/")) {
    walked = walked === "" ? part : `${walked}/${part}`;
    crumbs.push({ label: part, path: walked });
  }
  return crumbs;
}
