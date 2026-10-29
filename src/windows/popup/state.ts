/**
 * Pure popup state machine. See docs/APP_FLOW.md 6.2 and 6.7.
 *
 * The reducer knows nothing about IPC, timers or React. PopupApp drives it and
 * performs the side effects (lookup, checkCardContext, exportToAnki).
 */

import type {
  AnkiPayload,
  CardContext,
  Entry,
  ExportResult,
  IpcError,
  SelectionContext,
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

export interface PopupState {
  phase: PopupPhase;
  /** Last content phase; transient phases (editing/saving/saved/saveError) restore it. */
  contentPhase: ContentPhase | null;
  selection: SelectionContext | null;
  /** Id of the in-flight lookup; stale results whose id differs are ignored. */
  queryId: string | null;
  entries: Entry[];
  selectedEntryIndex: number;
  /** Reserved for M4 sense recommendations; always null in M2. */
  selectedSenseId: string | null;
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
  | { type: "entry-selected"; index: number }
  | { type: "sense-selected"; senseId: string | null }
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
  selectedSenseId: null,
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

/**
 * Build the export payload from the current state (docs/BACKEND_STRUCTURE.md 2.4).
 * The backend owns deck/model/field mapping, so the payload only carries content.
 */
export function buildAnkiPayload(state: PopupState): AnkiPayload {
  const sentence = state.draftSentence.trim();
  return {
    word: state.draftWord.trim() || state.selection?.text.trim() || "",
    definition_html: payloadDefinition(state),
    sentence: sentence.length > 0 ? sentence : null,
    source: payloadSource(state),
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
        selectedSenseId: null,
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
        error: null,
        contextWarning: null,
        saveResult: null,
      };

    case "lookup-succeeded": {
      if (state.phase !== "loading" || state.queryId !== action.queryId) {
        return state;
      }
      const hasEntries = action.entries.length > 0;
      const phase: ContentPhase = hasEntries ? "results" : "empty";
      return {
        ...state,
        phase,
        contentPhase: phase,
        entries: action.entries,
        selectedEntryIndex: 0,
        selectedSenseId: null,
        draftDefinition: hasEntries ? (action.entries[0]?.definition_html ?? "") : "",
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
        error: action.error,
      };

    case "entry-selected":
      if (state.phase !== "results") {
        return state;
      }
      if (action.index < 0 || action.index >= state.entries.length) {
        return state;
      }
      return {
        ...state,
        selectedEntryIndex: action.index,
        selectedSenseId: null,
        contextWarning: null,
      };

    case "sense-selected":
      if (!isEditingPhase(state) && state.phase !== "results") {
        return state;
      }
      return { ...state, selectedSenseId: action.senseId, contextWarning: null };

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
