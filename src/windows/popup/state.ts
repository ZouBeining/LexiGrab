/**
 * Pure popup state machine. See docs/APP_FLOW.md 6.2 and 6.7.
 *
 * The reducer knows nothing about IPC, timers or React. PopupApp drives it and
 * performs the side effects (lookup, query meta, remote events, sense events,
 * checkCardContext, exportToAnki).
 */

import type { RemoteLookupError, RemoteLookupResult } from "../../lib/events";
import type {
  AnkiPayload,
  CardContext,
  Entry,
  ExportResult,
  IpcError,
  QueryMeta,
  SelectionContext,
  SenseCandidate,
  SenseResolution,
} from "../../lib/types";

/** All phases the popup can be in. */
export type PopupPhase =
  | "capturing"
  | "loading"
  | "results"
  | "empty"
  | "queryError"
  | "captureError"
  | "editing"
  | "saving"
  | "saved"
  | "saveError";

/** Stable content phases the transient phases fall back to. */
export type ContentPhase = "results" | "empty" | "queryError" | "captureError";

export type DraftField = "word" | "definition" | "sentence";

/** Whether the current save started from the quick result view or from the editor. */
export type SaveOrigin = "results" | "editing";

/** Remote source currently answered from the lookup cache. */
export interface CachedSource {
  source_id: string;
  source_name: string;
}

export interface PopupState {
  phase: PopupPhase;
  /** Last content phase; transient phases (editing/saving/saved/saveError) restore it. */
  contentPhase: ContentPhase | null;
  selection: SelectionContext | null;
  /** Id of the in-flight lookup; stale results whose id differs are ignored. */
  queryId: string | null;
  entries: Entry[];
  selectedEntryIndex: number;
  /** Remote sources still answering this query (docs/BACKEND_STRUCTURE.md §2.5). */
  pendingRemote: number;
  /** Sources this query was served from cache; drives the "缓存" badge. */
  cachedSources: CachedSource[];
  /** Remote failures, shown collapsed under 诊断; never blocks the main result. */
  remoteErrors: RemoteLookupError[];
  /** Remote `(source_id, headword)` keys already appended, for dedupe. */
  remoteSeenKeys: string[];
  /** Latest counsel from the sense selector / LLM (M4). */
  sense: SenseResolution | null;
  /** User- or recommendation-picked sense id; drives the save payload. */
  selectedSenseId: string | null;
  /** True once the user pressed "使用此释义"; the AI text is being saved. */
  aiInUse: boolean;
  /** Once true, late sense events never replace the user's explicit choice. */
  senseChoiceMade: boolean;
  draftWord: string;
  draftDefinition: string;
  draftSentence: string;
  saveResult: ExportResult | null;
  contextWarning: CardContext | null;
  error: IpcError | null;
  saveOrigin: SaveOrigin;
}

export type PopupAction =
  | { type: "selection-received"; selection: SelectionContext; queryId: string }
  /** Backend re-sends the same selection with a filled sentence after the popup shows. */
  | { type: "selection-context-updated"; selection: SelectionContext }
  | { type: "capture-failed"; error: IpcError }
  | { type: "lookup-restarted"; queryId: string }
  | { type: "lookup-succeeded"; queryId: string; entries: Entry[] }
  | { type: "lookup-failed"; queryId: string; error: IpcError }
  | { type: "query-meta-received"; queryId: string; meta: QueryMeta }
  | { type: "remote-result"; result: RemoteLookupResult }
  | { type: "remote-error"; error: RemoteLookupError }
  | { type: "sense-received"; resolution: SenseResolution }
  | { type: "entry-selected"; index: number }
  | { type: "sense-selected"; senseId: string | null }
  | { type: "ai-in-use-requested" }
  | { type: "edit-requested" }
  | { type: "draft-changed"; field: DraftField; value: string }
  | { type: "edit-cancelled" }
  | { type: "save-requested" }
  | { type: "context-warning"; warning: CardContext }
  | { type: "context-warning-dismissed" }
  | { type: "save-confirmed" }
  | { type: "save-succeeded"; result: ExportResult }
  | { type: "save-failed"; error: IpcError }
  | { type: "back-to-results" }
  | { type: "reset" };

export const initialState: PopupState = {
  phase: "capturing",
  contentPhase: null,
  selection: null,
  queryId: null,
  entries: [],
  selectedEntryIndex: 0,
  pendingRemote: 0,
  cachedSources: [],
  remoteErrors: [],
  remoteSeenKeys: [],
  sense: null,
  selectedSenseId: null,
  aiInUse: false,
  senseChoiceMade: false,
  draftWord: "",
  draftDefinition: "",
  draftSentence: "",
  saveResult: null,
  contextWarning: null,
  error: null,
  saveOrigin: "results",
};

/** Entry currently shown in the quick view, if any. */
export function activeEntry(state: PopupState): Entry | null {
  return state.entries[state.selectedEntryIndex] ?? null;
}

/** True while the content (result/empty/error/edit) should stay on screen. */
export function isEditingPhase(state: PopupState): boolean {
  return state.phase === "editing" || (state.phase === "saveError" && state.saveOrigin === "editing");
}

/** Look up a candidate by id inside a resolution; unknown ids are rejected. */
export function candidateById(
  resolution: SenseResolution | null,
  senseId: string | null,
): SenseCandidate | null {
  if (!resolution || !senseId) {
    return null;
  }
  return resolution.candidates.find((candidate) => candidate.id === senseId) ?? null;
}

/** Candidate that currently drives the save payload (user choice first). */
export function selectedCandidate(state: PopupState): SenseCandidate | null {
  return candidateById(state.sense, state.selectedSenseId);
}

/** Recommended candidate even after the user overrides; always a valid candidate. */
function recommendedCandidate(state: PopupState): SenseCandidate | null {
  return candidateById(state.sense, state.sense?.selected_sense_id ?? null);
}

/** Entry index that should carry the "推荐" marker in the entry list. */
export function highlightedEntryIndex(state: PopupState): number | null {
  const candidate = selectedCandidate(state) ?? recommendedCandidate(state);
  if (!candidate) {
    return null;
  }
  if (candidate.entry_index < 0 || candidate.entry_index >= state.entries.length) {
    return null;
  }
  return candidate.entry_index;
}

/** True when the popup has an original sentence for sense assistance. */
export function hasSentence(state: PopupState): boolean {
  const sentence = state.draftSentence.trim() || state.selection?.sentence?.trim() || "";
  return sentence.length > 0;
}

/**
 * AI card shows for low confidence, or when a "selected" resolution points at a
 * sense that is not in the candidate set (docs/APP_FLOW.md §6.6).
 */
export function shouldShowAiCard(state: PopupState): boolean {
  const sense = state.sense;
  if (!sense?.ai) {
    return false;
  }
  if (sense.status === "low_confidence" || sense.status === "failed") {
    return true;
  }
  if (sense.status === "selected") {
    return candidateById(sense, sense.selected_sense_id) === null;
  }
  return false;
}

/** Escape model output before wrapping it in the exported definition HTML. */
export function escapeHtml(text: string): string {
  return text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

/** Anki definition HTML for the AI text: pure text wrapped in a single div. */
export function aiDefinitionHtml(text: string): string {
  return `<div>${escapeHtml(text)}</div>`;
}

/** Main save button copy (docs/APP_FLOW.md §6.6). */
export function saveButtonLabel(state: PopupState): string {
  return state.selectedSenseId !== null || state.aiInUse ? "保存所选义项" : "保存全部释义";
}

/** Source name for the Anki payload: an empty source means "未知来源" on the backend. */
function payloadSource(state: PopupState): string {
  return activeEntry(state)?.source_name ?? "";
}

/** Definition HTML for the payload: live entry in quick mode, draft in edit mode. */
function payloadDefinition(state: PopupState): string {
  if (isEditingPhase(state)) {
    return state.draftDefinition;
  }
  return activeEntry(state)?.definition_html ?? state.draftDefinition;
}

/** Word for the payload. */
function payloadWord(state: PopupState): string {
  return state.draftWord.trim() || state.selection?.text.trim() || "";
}

/** Sentence for the payload; blank becomes null. */
function payloadSentence(state: PopupState): string | null {
  const sentence = state.draftSentence.trim();
  return sentence.length > 0 ? sentence : null;
}

/**
 * Build the export payload from the current state (docs/BACKEND_STRUCTURE.md 2.4).
 * The backend owns deck/model/field mapping, so the payload only carries content.
 *
 * Resolution order: AI text (user adopted it) > selected sense > whole entry.
 */
export function buildAnkiPayload(state: PopupState): AnkiPayload {
  const word = payloadWord(state);
  const sentence = payloadSentence(state);

  if (state.aiInUse && state.sense?.ai) {
    const ai = state.sense.ai;
    return {
      word,
      definition_html: isEditingPhase(state)
        ? state.draftDefinition
        : aiDefinitionHtml(ai.definition_text),
      sentence,
      source: `AI · ${ai.model}`,
      tags: ["ai-generated"],
    };
  }

  const sense = selectedCandidate(state);
  if (sense) {
    return {
      word,
      definition_html: isEditingPhase(state) ? state.draftDefinition : sense.definition_html,
      sentence,
      source: sense.source_name,
      tags: [],
    };
  }

  return {
    word,
    definition_html: payloadDefinition(state),
    sentence,
    source: payloadSource(state),
    tags: [],
  };
}

/** Word shown in the header; falls back to the selection text. */
export function displayWord(state: PopupState): string {
  return state.draftWord || state.selection?.text || "";
}

/** Reset the per-capture fields when a new selection arrives. */
function resetForSelection(selection: SelectionContext, queryId: string): PopupState {
  return {
    ...initialState,
    phase: "loading",
    selection,
    queryId,
    draftWord: selection.text.trim(),
    draftSentence: selection.sentence ?? "",
  };
}

function restoreContentPhase(state: PopupState): ContentPhase {
  if (state.contentPhase) {
    return state.contentPhase;
  }
  return state.selection ? "results" : "captureError";
}

/** Seed the draft from the selected entry when entering the editor from results. */
function seedDraft(state: PopupState): Pick<PopupState, "draftWord" | "draftDefinition" | "draftSentence"> {
  const entry = activeEntry(state);
  return {
    draftWord: state.draftWord || state.selection?.text.trim() || "",
    draftDefinition: entry?.definition_html ?? "",
    draftSentence: state.draftSentence || state.selection?.sentence || "",
  };
}

/** Deduplicate entries by source name + headword, keeping the first occurrence. */
function mergeEntries(primary: Entry[], extra: Entry[]): Entry[] {
  const seen = new Set(primary.map((entry) => `${entry.source_name}\u0000${entry.headword}`));
  const merged = [...primary];
  for (const entry of extra) {
    const key = `${entry.source_name}\u0000${entry.headword}`;
    if (!seen.has(key)) {
      seen.add(key);
      merged.push(entry);
    }
  }
  return merged;
}

/** Add a cached source once, preserving the first observed name. */
function addCachedSource(sources: CachedSource[], source: CachedSource): CachedSource[] {
  if (sources.some((item) => item.source_id === source.source_id)) {
    return sources;
  }
  return [...sources, source];
}

export function popupReducer(state: PopupState, action: PopupAction): PopupState {
  switch (action.type) {
    case "selection-received":
      return resetForSelection(action.selection, action.queryId);

    case "selection-context-updated": {
      const current = state.selection;
      if (!current || current.text !== action.selection.text) {
        return state;
      }
      // Only refresh the sentence. Editing drafts and in-flight phases stay untouched
      // so a late context event never discards user input or save results.
      const editing = isEditingPhase(state);
      return {
        ...state,
        selection: action.selection,
        draftSentence: editing ? state.draftSentence : action.selection.sentence ?? state.draftSentence,
      };
    }

    case "capture-failed":
      return {
        ...state,
        phase: "captureError",
        contentPhase: "captureError",
        selection: null,
        queryId: null,
        entries: [],
        selectedEntryIndex: 0,
        pendingRemote: 0,
        cachedSources: [],
        remoteErrors: [],
        remoteSeenKeys: [],
        sense: null,
        selectedSenseId: null,
        aiInUse: false,
        senseChoiceMade: false,
        draftWord: "",
        draftDefinition: "",
        draftSentence: "",
        saveResult: null,
        contextWarning: null,
        error: action.error,
      };

    case "lookup-restarted":
      if (state.phase !== "queryError") {
        return state;
      }
      return {
        ...state,
        phase: "loading",
        queryId: action.queryId,
        entries: [],
        selectedEntryIndex: 0,
        pendingRemote: 0,
        cachedSources: [],
        remoteErrors: [],
        remoteSeenKeys: [],
        sense: null,
        selectedSenseId: null,
        aiInUse: false,
        senseChoiceMade: false,
        error: null,
        contextWarning: null,
        saveResult: null,
      };

    case "lookup-succeeded": {
      if (state.phase !== "loading" || state.queryId !== action.queryId) {
        return state;
      }
      // Remote events may fire before this promise resolves; keep anything already
      // appended so a fast remote source is not dropped.
      const entries = mergeEntries(action.entries, state.entries);
      const hasEntries = entries.length > 0;
      const phase: ContentPhase = hasEntries ? "results" : "empty";
      const first = entries[0] as Entry | undefined;
      return {
        ...state,
        phase,
        contentPhase: phase,
        entries,
        selectedEntryIndex: 0,
        sense: null,
        selectedSenseId: null,
        aiInUse: false,
        senseChoiceMade: false,
        draftDefinition: first?.definition_html ?? "",
        error: null,
      };
    }

    case "lookup-failed":
      if (state.phase !== "loading" || state.queryId !== action.queryId) {
        return state;
      }
      return {
        ...state,
        phase: "queryError",
        contentPhase: "queryError",
        entries: [],
        selectedEntryIndex: 0,
        pendingRemote: 0,
        sense: null,
        selectedSenseId: null,
        aiInUse: false,
        senseChoiceMade: false,
        error: action.error,
      };

    case "query-meta-received":
      if (state.queryId === null || state.queryId !== action.queryId) {
        return state;
      }
      return {
        ...state,
        pendingRemote: Math.max(0, action.meta.pending_remote),
        cachedSources: action.meta.cached_sources,
      };

    case "remote-result": {
      const result = action.result;
      if (state.queryId === null || state.queryId !== result.query_id) {
        return state;
      }
      const seenRemote = new Set(state.remoteSeenKeys);
      const accepted: Entry[] = [];
      for (const entry of result.entries) {
        const remoteKey = `${result.source_id}\u0000${entry.headword}`;
        const nameKey = `${entry.source_name}\u0000${entry.headword}`;
        const duplicate =
          seenRemote.has(remoteKey) ||
          state.entries.some(
            (existing) => `${existing.source_name}\u0000${existing.headword}` === nameKey,
          );
        if (!duplicate) {
          accepted.push(entry);
          seenRemote.add(remoteKey);
        }
      }
      const entries = mergeEntries(state.entries, accepted);
      const hasEntries = entries.length > 0;
      const fillsEmptyResults =
        hasEntries && (state.phase === "empty" || state.phase === "loading");
      const phase = state.phase === "empty" && hasEntries ? "results" : state.phase;
      const contentPhase = fillsEmptyResults ? "results" : state.contentPhase;
      let cachedSources = state.cachedSources;
      if (result.cached) {
        cachedSources = addCachedSource(cachedSources, {
          source_id: result.source_id,
          source_name: result.source_name,
        });
      }
      const firstRemote = accepted[0] as Entry | undefined;
      return {
        ...state,
        phase,
        contentPhase,
        entries,
        remoteSeenKeys: [...seenRemote],
        pendingRemote: Math.max(0, state.pendingRemote - 1),
        cachedSources,
        // A synchronously-empty query may be filled by a remote source; seed the
        // quick-view definition so saving without switching entries still works.
        draftDefinition:
          state.entries.length === 0 && firstRemote
            ? firstRemote.definition_html
            : state.draftDefinition,
      };
    }

    case "remote-error":
      if (state.queryId === null || state.queryId !== action.error.query_id) {
        return state;
      }
      return {
        ...state,
        pendingRemote: Math.max(0, state.pendingRemote - 1),
        remoteErrors: [...state.remoteErrors, action.error].slice(-10),
      };

    case "sense-received": {
      const resolution = action.resolution;
      if (state.queryId === null || state.queryId !== resolution.query_id) {
        return state;
      }
      const next: PopupState = { ...state, sense: resolution };
      if (!state.senseChoiceMade) {
        const recommended =
          resolution.status === "selected"
            ? candidateById(resolution, resolution.selected_sense_id)
            : null;
        // Only a high-confidence recommendation auto-selects; ambiguous/low
        // confidence leave the whole entry in place for the user to confirm.
        next.selectedSenseId = recommended?.id ?? null;
        next.aiInUse = false;
        if (
          recommended &&
          recommended.entry_index >= 0 &&
          recommended.entry_index < state.entries.length
        ) {
          next.selectedEntryIndex = recommended.entry_index;
        }
      }
      return next;
    }

    case "entry-selected":
      if (state.phase !== "results") {
        return state;
      }
      if (action.index < 0 || action.index >= state.entries.length) {
        return state;
      }
      // Picking an entry is an explicit manual choice; freeze auto-selection.
      return {
        ...state,
        selectedEntryIndex: action.index,
        selectedSenseId: null,
        aiInUse: false,
        senseChoiceMade: true,
        contextWarning: null,
      };

    case "sense-selected": {
      if (!isEditingPhase(state) && state.phase !== "results") {
        return state;
      }
      if (action.senseId === null) {
        return {
          ...state,
          selectedSenseId: null,
          aiInUse: false,
          senseChoiceMade: true,
          contextWarning: null,
        };
      }
      const candidate = candidateById(state.sense, action.senseId);
      if (!candidate) {
        return state;
      }
      const index =
        candidate.entry_index >= 0 && candidate.entry_index < state.entries.length
          ? candidate.entry_index
          : state.selectedEntryIndex;
      return {
        ...state,
        selectedSenseId: candidate.id,
        selectedEntryIndex: index,
        aiInUse: false,
        senseChoiceMade: true,
        contextWarning: null,
      };
    }

    case "ai-in-use-requested": {
      const ai = state.sense?.ai;
      if (!ai || state.phase !== "results") {
        return state;
      }
      // Adopting the AI card opens the editor with escaped plain text, still editable.
      return {
        ...state,
        phase: "editing",
        draftWord: state.draftWord || state.selection?.text.trim() || "",
        draftDefinition: aiDefinitionHtml(ai.definition_text),
        draftSentence: state.draftSentence || state.selection?.sentence || "",
        aiInUse: true,
        selectedSenseId: null,
        senseChoiceMade: true,
        error: null,
        contextWarning: null,
      };
    }

    case "edit-requested": {
      if (state.phase === "saveError") {
        // Retry editing from either save origin; drafts are already seeded/edited.
        return { ...state, phase: "editing", error: null, contextWarning: null };
      }
      if (state.phase !== "results" && state.phase !== "empty" && state.phase !== "queryError" && state.phase !== "captureError") {
        return state;
      }
      const draft = seedDraft(state);
      return {
        ...state,
        phase: "editing",
        ...draft,
        error: null,
        contextWarning: null,
      };
    }

    case "draft-changed":
      if (!isEditingPhase(state)) {
        return state;
      }
      if (action.field === "word") {
        return { ...state, draftWord: action.value };
      }
      if (action.field === "definition") {
        return { ...state, draftDefinition: action.value };
      }
      return { ...state, draftSentence: action.value };

    case "edit-cancelled":
      if (!isEditingPhase(state)) {
        return state;
      }
      return {
        ...state,
        phase: restoreContentPhase(state),
        contentPhase: restoreContentPhase(state),
        contextWarning: null,
        error: null,
      };

    case "save-requested":
      if (
        state.phase !== "results" &&
        state.phase !== "empty" &&
        state.phase !== "editing" &&
        state.phase !== "saveError"
      ) {
        return state;
      }
      return {
        ...state,
        phase: "saving",
        saveOrigin:
          state.phase === "editing"
            ? "editing"
            : state.phase === "saveError"
              ? state.saveOrigin
              : "results",
        contextWarning: null,
        error: null,
        saveResult: null,
      };

    case "context-warning":
      if (state.phase !== "saving") {
        return state;
      }
      return {
        ...state,
        phase: state.saveOrigin === "editing" ? "editing" : restoreContentPhase(state),
        contextWarning: action.warning,
        error: null,
      };

    case "context-warning-dismissed":
      if (!state.contextWarning) {
        return state;
      }
      return { ...state, contextWarning: null };

    case "save-confirmed":
      if (state.phase !== "saving" && !state.contextWarning) {
        return state;
      }
      return {
        ...state,
        phase: "saving",
        contextWarning: null,
        error: null,
      };

    case "save-succeeded":
      if (state.phase !== "saving") {
        return state;
      }
      return {
        ...state,
        phase: "saved",
        saveResult: action.result,
        contextWarning: null,
        error: null,
      };

    case "save-failed":
      if (state.phase !== "saving") {
        return state;
      }
      return {
        ...state,
        phase: "saveError",
        error: action.error,
        contextWarning: null,
      };

    case "back-to-results": {
      if (state.phase !== "saved" && state.phase !== "saveError") {
        return state;
      }
      const phase: ContentPhase = state.entries.length > 0 ? "results" : "empty";
      return {
        ...state,
        phase,
        contentPhase: phase,
        saveResult: null,
        contextWarning: null,
        error: null,
      };
    }

    case "reset":
      return initialState;

    default:
      return state;
  }
}
