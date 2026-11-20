// SPDX-License-Identifier: AGPL-3.0-only
import { describe, expect, it } from "vitest";
import type { RemoteLookupResult } from "../../lib/events";
import type {
  CardContext,
  Entry,
  IpcError,
  QueryMeta,
  SelectionContext,
  SenseCandidate,
  SenseResolution,
} from "../../lib/types";
import {
  buildAnkiPayload,
  hasSentence,
  initialState,
  popupReducer,
  saveButtonLabel,
  shouldShowAiCard,
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

const candidate = (overrides: Partial<SenseCandidate> = {}): SenseCandidate => ({
  id: "e0-s1",
  entry_index: 0,
  source_name: "Mini",
  headword: "serendipity",
  part_of_speech: "noun",
  definition_text: "意外发现珍奇事物的本领",
  definition_html: "<div>意外发现珍奇事物的本领</div>",
  ...overrides,
});

const remoteResult = (overrides: Partial<RemoteLookupResult> = {}): RemoteLookupResult => ({
  query_id: "q-1",
  source_id: "remote-1",
  source_name: "Remote",
  entries: [entry({ source_name: "Remote", definition_html: "<p>远程释义</p>" })],
  cached: false,
  elapsed_ms: 42,
  ...overrides,
});

const senseResolution = (overrides: Partial<SenseResolution> = {}): SenseResolution => ({
  query_id: "q-1",
  headword: "serendipity",
  status: "selected",
  selected_sense_id: "e0-s1",
  confidence: 0.86,
  selector_model: "jev-like",
  ai: null,
  error: null,
  candidates: [candidate()],
  ...overrides,
});

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

describe("popupReducer remote sources", () => {
  it("initializes pending remote count and cached sources from query meta", () => {
    const state = withResults();
    const meta: QueryMeta = {
      query_id: "q-1",
      cached_sources: [{ source_id: "remote-1", source_name: "Remote" }],
      pending_remote: 2,
    };

    const updated = popupReducer(state, { type: "query-meta-received", queryId: "q-1", meta });

    expect(updated.pendingRemote).toBe(2);
    expect(updated.cachedSources).toEqual([{ source_id: "remote-1", source_name: "Remote" }]);
  });

  it("ignores query meta for a stale query id", () => {
    const state = withResults();
    const meta: QueryMeta = {
      query_id: "q-0",
      cached_sources: [],
      pending_remote: 3,
    };

    const updated = popupReducer(state, { type: "query-meta-received", queryId: "q-0", meta });

    expect(updated).toBe(state);
  });

  it("appends remote entries, dedupes repeated payloads and decrements pending", () => {
    const state = run(withResults(), {
      type: "query-meta-received",
      queryId: "q-1",
      meta: { query_id: "q-1", cached_sources: [], pending_remote: 1 },
    });

    const first = popupReducer(state, { type: "remote-result", result: remoteResult() });
    expect(first.entries).toHaveLength(2);
    expect(first.entries[1]?.source_name).toBe("Remote");
    expect(first.pendingRemote).toBe(0);

    const duplicate = popupReducer(first, { type: "remote-result", result: remoteResult() });
    expect(duplicate.entries).toHaveLength(2);
    expect(duplicate.pendingRemote).toBe(0);
  });

  it("marks a cached remote source with the cache badge data", () => {
    const state = withResults();
    const updated = popupReducer(state, {
      type: "remote-result",
      result: remoteResult({ cached: true }),
    });

    expect(updated.cachedSources).toEqual([{ source_id: "remote-1", source_name: "Remote" }]);
  });

  it("fills an empty result with a late remote entry", () => {
    const empty = withResults([]);
    expect(empty.phase).toBe("empty");

    const updated = popupReducer(empty, { type: "remote-result", result: remoteResult() });

    expect(updated.phase).toBe("results");
    expect(updated.contentPhase).toBe("results");
    expect(updated.draftDefinition).toBe("<p>远程释义</p>");
  });

  it("records a remote error without touching the main result", () => {
    const state = run(withResults(), {
      type: "query-meta-received",
      queryId: "q-1",
      meta: { query_id: "q-1", cached_sources: [], pending_remote: 1 },
    });

    const failed = popupReducer(state, {
      type: "remote-error",
      error: {
        query_id: "q-1",
        source_id: "remote-1",
        code: "REMOTE_TIMEOUT",
        message: "远程源超时",
      },
    });

    expect(failed.entries).toEqual(state.entries);
    expect(failed.phase).toBe("results");
    expect(failed.error).toBeNull();
    expect(failed.pendingRemote).toBe(0);
    expect(failed.remoteErrors).toHaveLength(1);
  });

  it("ignores remote results and errors for a stale query id", () => {
    const state = withResults();
    const loading = popupReducer(state, {
      type: "selection-received",
      selection: selection({ text: "bank" }),
      queryId: "q-2",
    });

    expect(popupReducer(loading, { type: "remote-result", result: remoteResult() })).toBe(loading);
    expect(
      popupReducer(loading, {
        type: "remote-error",
        error: { query_id: "q-1", source_id: "remote-1", code: "X", message: "失败" },
      }),
    ).toBe(loading);
  });
});

describe("popupReducer sense assistance", () => {
  const twoSenses = [
    candidate(),
    candidate({
      id: "e0-s2",
      definition_text: "河岸",
      definition_html: "<div>河岸</div>",
    }),
  ];

  it("auto-selects the high-confidence recommendation and saves that sense", () => {
    const state = run(withResults(), {
      type: "sense-received",
      resolution: senseResolution({ candidates: twoSenses }),
    });

    expect(state.selectedSenseId).toBe("e0-s1");
    expect(state.aiInUse).toBe(false);
    expect(saveButtonLabel(state)).toBe("保存所选义项");

    const payload = buildAnkiPayload(state);
    expect(payload.definition_html).toBe("<div>意外发现珍奇事物的本领</div>");
    expect(payload.source).toBe("Mini");
    expect(payload.tags).toEqual([]);
  });

  it("does not auto-select for ambiguous confidence", () => {
    const state = run(withResults(), {
      type: "sense-received",
      resolution: senseResolution({ status: "ambiguous", confidence: 0.62 }),
    });

    expect(state.selectedSenseId).toBeNull();
    expect(saveButtonLabel(state)).toBe("保存全部释义");
    expect(buildAnkiPayload(state).definition_html).toBe(entry().definition_html);
  });

  it("does not auto-select for low confidence", () => {
    const state = run(withResults(), {
      type: "sense-received",
      resolution: senseResolution({ status: "low_confidence", selected_sense_id: null, confidence: 0.3 }),
    });

    expect(state.selectedSenseId).toBeNull();
    expect(saveButtonLabel(state)).toBe("保存全部释义");
  });

  it("keeps a user-picked sense when a later recommendation arrives", () => {
    const recommended = run(withResults(), {
      type: "sense-received",
      resolution: senseResolution({ candidates: twoSenses }),
    });
    const userPicked = popupReducer(recommended, { type: "sense-selected", senseId: "e0-s2" });
    expect(userPicked.selectedSenseId).toBe("e0-s2");

    const late = popupReducer(userPicked, {
      type: "sense-received",
      resolution: senseResolution({ candidates: twoSenses, confidence: 0.95 }),
    });

    expect(late.selectedSenseId).toBe("e0-s2");
    expect(buildAnkiPayload(late).definition_html).toBe("<div>河岸</div>");
  });

  it("keeps a user-picked entry when a later recommendation arrives", () => {
    const state = popupReducer(withResults(), { type: "entry-selected", index: 0 });
    const late = popupReducer(state, {
      type: "sense-received",
      resolution: senseResolution(),
    });

    expect(late.selectedSenseId).toBeNull();
  });

  it("ignores a sense result for a stale query id", () => {
    const state = withResults();
    const stale = popupReducer(state, {
      type: "sense-received",
      resolution: senseResolution({ query_id: "q-0" }),
    });

    expect(stale).toBe(state);
  });

  it("keeps the AI card unselected until the user adopts it", () => {
    const ai = {
      definition_text: "river <bank> & shore",
      part_of_speech: "noun",
      model: "llama3",
      confidence: 0.6,
      based_on_sense_id: null,
      generated_at: 1,
    };
    const state = run(withResults(), {
      type: "sense-received",
      resolution: senseResolution({ status: "low_confidence", selected_sense_id: null, ai }),
    });

    expect(state.aiInUse).toBe(false);
    expect(state.selectedSenseId).toBeNull();
    expect(shouldShowAiCard(state)).toBe(true);
    expect(buildAnkiPayload(state).source).toBe("Mini");

    const adopted = popupReducer(state, { type: "ai-in-use-requested" });
    expect(adopted.phase).toBe("editing");
    expect(adopted.aiInUse).toBe(true);
    expect(adopted.draftDefinition).toBe("<div>river &lt;bank&gt; &amp; shore</div>");

    const payload = buildAnkiPayload(adopted);
    expect(payload.source).toBe("AI · llama3");
    expect(payload.tags).toEqual(["ai-generated"]);
    expect(payload.definition_html).toBe("<div>river &lt;bank&gt; &amp; shore</div>");
  });

  it("hides the AI card when a selected resolution has a matching candidate", () => {
    const state = run(withResults(), {
      type: "sense-received",
      resolution: senseResolution({
        ai: {
          definition_text: "AI 释义",
          part_of_speech: null,
          model: "llama3",
          confidence: null,
          based_on_sense_id: "e0-s1",
          generated_at: 1,
        },
      }),
    });

    expect(shouldShowAiCard(state)).toBe(false);
  });
});

describe("hasSentence", () => {
  it("is false when neither the selection nor the draft carries a sentence", () => {
    const state = withResults([entry()], selection({ sentence: null }));

    expect(hasSentence(state)).toBe(false);
  });

  it("is true for a captured sentence", () => {
    expect(hasSentence(withResults())).toBe(true);
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
      tags: [],
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
      tags: [],
    });
  });
});
