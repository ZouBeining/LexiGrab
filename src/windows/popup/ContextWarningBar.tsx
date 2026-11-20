// SPDX-License-Identifier: AGPL-3.0-only
import { Button } from "../../components/Button";
import type { CardContext } from "../../lib/types";

export interface ContextWarningBarProps {
  warning: CardContext;
  /** Disable actions while a save is already in flight. */
  disabled?: boolean;
  onSaveAnyway: () => void;
  onSwitchEntry: () => void;
}

/**
 * Same-word/same-sentence/same-definition hint (docs/APP_FLOW.md §6.7).
 * Advisory only: "仍然保存" exports a second card, "换义项" goes back to results.
 */
export function ContextWarningBar({
  warning,
  disabled = false,
  onSaveAnyway,
  onSwitchEntry,
}: ContextWarningBarProps) {
  return (
    <div
      role="alert"
      className="mx-4 mb-2 shrink-0 rounded-lg border border-warning/40 bg-canvas-soft px-3 py-2"
    >
      <p className="text-xs leading-5 text-ink">
        此语境已存过
        {warning.note_id !== null ? `（note ${warning.note_id}）` : ""}
        ，可换义项或仍然保存。
      </p>
      <div className="mt-2 flex items-center justify-end gap-2">
        <Button size="sm" variant="secondary" onClick={onSwitchEntry} disabled={disabled}>
          换义项
        </Button>
        <Button size="sm" onClick={onSaveAnyway} disabled={disabled}>
          仍然保存
        </Button>
      </div>
    </div>
  );
}
