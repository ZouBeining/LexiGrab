import { describe, expect, it } from "vitest";
import type { CardContext, Entry, IpcError, SelectionContext } from "../../lib/types";
import {
  buildAnkiPayload,
  initialState,
  popupReducer,
  type PopupAction,
  type PopupState,
} from "./state";

const selection = (overrides: Partial<SelectionContext> = {}): SelectionContext => ({
  text: "serendipity",
  sentence: "It was pure serendipity that we met.",
  url: null,
  title: null,
  domain: null,
  anchor: null,
  foreground_hwnd: 1024,
  app_name: "notepad.exe",
  method: "uia",
  captured_at_ms: 1_700_000_000_000,
  elapsed_ms: 12,
  ...overrides,
});

const entry = (overrides: Partial<Entry> = {}): Entry => ({
  headword: "serendipity",
  definition_html: "<div>意外发现珍奇事物的本领</div>",
  phonetic: "/ˌserənˈdɪpəti/",
  resources: [],
  source_name: "Mini",
  ...overrides,
});

const ipcError = (code: string, message = "出错了"): IpcError => ({ code, message, detail: null });

const warning: CardContext = { seen: true, note_id: 42, created_at: 1_700_000_000_000 };

const run = (state: PopupState, ...actions: PopupAction[]): PopupState =>
  actions.reduce(popupReducer, state);

/** Selection received, first lookup finished with the given entries. */
function withResults(entries: Entry[] = [entry()], sel = selection()): PopupState {
  return run(
    initialState,
    { type: "selection-received", selection: sel, queryId: "q-1" },
    { type: "lookup-succeeded", queryId: "q-1", entries },
  );
}

describe("popupReducer capture -> lookup", () => {
  it("starts loading when a selection event arrives", () => {
    const sel = selection({ text: "  serendipity  ", sentence: null });
    const state = popupReducer(initialState, {
      type: "selection-received",
      selection: sel,
      queryId: "q-1",
    });

    expect(state.phase).toBe("loading");
    expect(state.queryId).toBe("q-1");
    expect(state.selection).toBe(sel);
    expect(state.draftWord).toBe("serendipity");
    expect(state.draftSentence).toBe("");
    expect(state.contentPhase).toBeNull();
  });

  it("enters results when the lookup returns entries", () => {
    const first = entry();
    const state = withResults([first, entry({ headword: "serendipity (2)" })]);

    expect(state.phase).toBe("results");
    expect(state.contentPhase).toBe("results");
    expect(state.entries).toHaveLength(2);
    expect(state.selectedEntryIndex).toBe(0);
    expect(state.draftDefinition).toBe(first.definition_html);
  });

  it("enters empty when the lookup returns no entries", () => {
    const state = withResults([]);

    expect(state.phase).toBe("empty");
    expect(state.contentPhase).toBe("empty");
    expect(state.entries).toEqual([]);
  });

  it("enters queryError when the lookup fails and can retry", () => {
    const failed = run(
      initialState,
      { type: "selection-received", selection: selection(), queryId: "q-1" },
      { type: "lookup-failed", queryId: "q-1", error: ipcError("DICT_BUSY") },
    );

    expect(failed.phase).toBe("queryError");
    expect(failed.error?.code).toBe("DICT_BUSY");

    const retried = run(
      failed,
      { type: "lookup-restarted", queryId: "q-2" },
      { type: "lookup-succeeded", queryId: "q-2", entries: [entry()] },
    );

    expect(retried.phase).toBe("results");
    expect(retried.queryId).toBe("q-2");
    expect(retried.error).toBeNull();
  });

  it("enters captureError when capture fails", () => {
    const state = popupReducer(initialState, {
      type: "capture-failed",
      error: ipcError("QUERY_EMPTY", "没有捕获到文本"),
    });

    expect(state.phase).toBe("captureError");
    expect(state.contentPhase).toBe("captureError");
    expect(state.selection).toBeNull();
    expect(state.error?.message).toBe("没有捕获到文本");
  });
});

describe("popupReducer stale query ids", () => {
  it("ignores a lookup result whose query id is not the active one", () => {
    const loading = run(
      initialState,
      { type: "selection-received", selection: selection(), queryId: "q-2" },
    );

    const stale = run(loading, {
      type: "lookup-succeeded",
      queryId: "q-1",
      entries: [entry()],
    });

    expect(stale).toBe(loading);
    expect(stale.phase).toBe("loading");
    expect(stale.entries).toEqual([]);
  });

  it("ignores results from a previous query after a new selection starts", () => {
    const firstResults = withResults();

    const secondLoading = popupReducer(firstResults, {
      type: "selection-received",
      selection: selection({ text: "bank" }),
      queryId: "q-2",
    });

    const stale = popupReducer(secondLoading, {
      type: "lookup-succeeded",
      queryId: "q-1",
      entries: [entry()],
    });

    expect(stale.phase).toBe("loading");
    expect(stale.entries).toEqual([]);

    const fresh = popupReducer(stale, {
      type: "lookup-succeeded",
      queryId: "q-2",
      entries: [entry({ headword: "bank" })],
    });
    expect(fresh.phase).toBe("results");
    expect(fresh.entries[0]?.headword).toBe("bank");
  });

  it("ignores a late lookup failure for a stale query id", () => {
    const loading = run(
      initialState,
      { type: "selection-received", selection: selection(), queryId: "q-2" },
    );

    const stale = run(loading, {
      type: "lookup-failed",
      queryId: "q-1",
      error: ipcError("INTERNAL"),
    });

    expect(stale.phase).toBe("loading");
    expect(stale.error).toBeNull();
  });
});

describe("popupReducer sentence context update", () => {
  it("silently fills the sentence for the same selection text", () => {
    const state = withResults();
    expect(state.draftSentence).toBe("It was pure serendipity that we met.");

    const updated = popupReducer(
      { ...state, draftSentence: "" },
      {
        type: "selection-context-updated",
        selection: selection({ sentence: "A later captured sentence.", captured_at_ms: 2 }),
      },
    );

    expect(updated.phase).toBe("results");
    expect(updated.entries).toEqual(state.entries);
    expect(updated.selection?.sentence).toBe("A later captured sentence.");
    expect(updated.draftSentence).toBe("A later captured sentence.");
  });

  it("does not overwrite the sentence draft while editing", () => {
    const editing = run(withResults(), { type: "edit-requested" }, {
      type: "draft-changed",
      field: "sentence",
      value: "User edited sentence.",
    });

    const updated = popupReducer(editing, {
      type: "selection-context-updated",
      selection: selection({ sentence: "Backend sentence." }),
    });

    expect(updated.phase).toBe("editing");
    expect(updated.draftSentence).toBe("User edited sentence.");
    expect(updated.selection?.sentence).toBe("Backend sentence.");
  });

  it("ignores a context update for a different word", () => {
    const state = withResults();

    const updated = popupReducer(state, {
      type: "selection-context-updated",
      selection: selection({ text: "bank", sentence: "Another sentence." }),
    });

    expect(updated).toBe(state);
  });
});

describe("popupReducer edit mode", () => {
  it("seeds the editor with the selected entry and returns on cancel", () => {
    const state = withResults([entry(), entry({ definition_html: "<p>第二条</p>" })]);
    const selected = popupReducer(state, { type: "entry-selected", index: 1 });

    const editing = popupReducer(selected, { type: "edit-requested" });
    expect(editing.phase).toBe("editing");
    expect(editing.draftWord).toBe("serendipity");
    expect(editing.draftDefinition).toBe("<p>第二条</p>");
    expect(editing.draftSentence).toBe("It was pure serendipity that we met.");

    const changed = run(
      editing,
      { type: "draft-changed", field: "definition", value: "<p>手动修改</p>" },
      { type: "draft-changed", field: "sentence", value: "Only the sentence." },
    );
    expect(changed.draftDefinition).toBe("<p>手动修改</p>");
    expect(changed.draftSentence).toBe("Only the sentence.");

    const cancelled = popupReducer(changed, { type: "edit-cancelled" });
    expect(cancelled.phase).toBe("results");
    expect(cancelled.entries).toHaveLength(2);
  });

  it("opens the editor from empty and keeps an empty definition", () => {
    const empty = withResults([]);
    const editing = popupReducer(empty, { type: "edit-requested" });

    expect(editing.phase).toBe("editing");
    expect(editing.draftDefinition).toBe("");
    expect(editing.draftWord).toBe("serendipity");
  });

  it("ignores draft changes outside edit mode", () => {
    const state = withResults();
    const changed = popupReducer(state, {
      type: "draft-changed",
      field: "definition",
      value: "<p>x</p>",
    });

    expect(changed).toBe(state);
  });

  it("ignores out-of-range entry selection", () => {
    const state = withResults();
    expect(popupReducer(state, { type: "entry-selected", index: 3 })).toBe(state);
    expect(popupReducer(state, { type: "entry-selected", index: -1 })).toBe(state);
  });
});

describe("popupReducer save flow", () => {
  it("moves through saving -> saved -> back to results", () => {
    const state = withResults();

    const saving = popupReducer(state, { type: "save-requested" });
    expect(saving.phase).toBe("saving");
    expect(saving.saveOrigin).toBe("results");

    const saved = popupReducer(saving, {
      type: "save-succeeded",
      result: { note_id: 12_345, exact_context_seen: false },
    });
    expect(saved.phase).toBe("saved");
    expect(saved.saveResult?.note_id).toBe(12_345);

    const back = popupReducer(saved, { type: "back-to-results" });
    expect(back.phase).toBe("results");
    expect(back.entries).toHaveLength(1);
    expect(back.saveResult).toBeNull();
  });

  it("keeps the editor drafts after a failed save and lets the user retry", () => {
    const editing = run(
      withResults(),
      { type: "edit-requested" },
      { type: "draft-changed", field: "definition", value: "<p>草稿</p>" },
    );

    const failed = run(
      editing,
      { type: "save-requested" },
      { type: "save-failed", error: ipcError("ANKI_UNREACHABLE", "Anki 未运行") },
    );

    expect(failed.phase).toBe("saveError");
    expect(failed.error?.code).toBe("ANKI_UNREACHABLE");
    expect(failed.draftDefinition).toBe("<p>草稿</p>");

    const backToEditing = popupReducer(failed, { type: "edit-requested" });
    expect(backToEditing.phase).toBe("editing");
    expect(backToEditing.draftDefinition).toBe("<p>草稿</p>");
    expect(backToEditing.error).toBeNull();
  });

  it("shows the same-context warning, then saves anyway on confirmation", () => {
    const saving = popupReducer(withResults(), { type: "save-requested" });
    const warned = popupReducer(saving, { type: "context-warning", warning });

    expect(warned.phase).toBe("results");
    expect(warned.contextWarning).toEqual(warning);
    expect(warned.saveResult).toBeNull();

    const confirmed = popupReducer(warned, { type: "save-confirmed" });
    expect(confirmed.phase).toBe("saving");
    expect(confirmed.contextWarning).toBeNull();

    const saved = popupReducer(confirmed, {
      type: "save-succeeded",
      result: { note_id: 7, exact_context_seen: true },
    });
    expect(saved.phase).toBe("saved");
    expect(saved.saveResult?.exact_context_seen).toBe(true);
  });

  it("returns to the editor with drafts when the warning comes from edit mode", () => {
    const editing = run(
      withResults(),
      { type: "edit-requested" },
      { type: "draft-changed", field: "sentence", value: "编辑后的原句" },
    );

    const warned = run(
      editing,
      { type: "save-requested" },
      { type: "context-warning", warning },
    );

    expect(warned.phase).toBe("editing");
    expect(warned.contextWarning?.seen).toBe(true);
    expect(warned.draftSentence).toBe("编辑后的原句");

    const cancelled = popupReducer(warned, { type: "edit-cancelled" });
    expect(cancelled.phase).toBe("results");
    expect(cancelled.contextWarning).toBeNull();
  });

  it("dismisses the warning and keeps the results", () => {
    const warned = run(
      withResults(),
      { type: "save-requested" },
      { type: "context-warning", warning },
    );

    const dismissed = popupReducer(warned, { type: "context-warning-dismissed" });
    expect(dismissed.phase).toBe("results");
    expect(dismissed.contextWarning).toBeNull();
  });

  it("retries a quick-mode save failure without entering edit mode", () => {
    const failed = run(
      withResults(),
      { type: "save-requested" },
      { type: "save-failed", error: ipcError("ANKI_UNREACHABLE") },
    );

    expect(failed.phase).toBe("saveError");
    expect(failed.saveOrigin).toBe("results");

    const retry = popupReducer(failed, { type: "save-requested" });
    expect(retry.phase).toBe("saving");
    expect(retry.saveOrigin).toBe("results");
  });

  it("opens the editor from a quick-mode save failure", () => {
    const failed = run(
      withResults(),
      { type: "save-requested" },
      { type: "save-failed", error: ipcError("INTERNAL") },
    );

    const editing = popupReducer(failed, { type: "edit-requested" });
    expect(editing.phase).toBe("editing");
    expect(editing.draftDefinition).toBe(entry().definition_html);
    expect(editing.error).toBeNull();
  });

  it("resets to the initial capturing phase", () => {
    const state = withResults();
    const reset = popupReducer(state, { type: "reset" });

    expect(reset).toEqual(initialState);
    expect(reset.phase).toBe("capturing");
  });

  it("ignores save transitions from unrelated phases", () => {
    const loading = run(
      initialState,
      { type: "selection-received", selection: selection(), queryId: "q-1" },
    );

    expect(popupReducer(loading, { type: "save-requested" })).toBe(loading);
    expect(
      popupReducer(loading, { type: "save-succeeded", result: { note_id: 1, exact_context_seen: false } }),
    ).toBe(loading);
  });
});

describe("buildAnkiPayload", () => {
  it("uses the selected entry in quick mode", () => {
    const state = withResults([
      entry(),
      entry({ headword: "second", definition_html: "<p>第二个</p>", source_name: "Other" }),
    ]);
    const selected = popupReducer(state, { type: "entry-selected", index: 1 });

    expect(buildAnkiPayload(selected)).toEqual({
      word: "serendipity",
      definition_html: "<p>第二个</p>",
      sentence: "It was pure serendipity that we met.",
      source: "Other",
    });
  });

  it("uses edited drafts and nulls an empty sentence", () => {
    const editing = run(
      withResults([]),
      { type: "edit-requested" },
      { type: "draft-changed", field: "definition", value: "<p>自制释义</p>" },
      { type: "draft-changed", field: "sentence", value: "   " },
    );

    expect(buildAnkiPayload(editing)).toEqual({
      word: "serendipity",
      definition_html: "<p>自制释义</p>",
      sentence: null,
      source: "",
    });
  });
});
