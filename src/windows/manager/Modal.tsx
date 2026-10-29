import { useCallback, useEffect, useRef } from "react";
import type { ReactNode, RefObject } from "react";

const FOCUSABLE =
  'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

interface ModalProps {
  /** Accessible dialog name; the element with this id must exist inside children. */
  labelledBy: string;
  onClose: () => void;
  /** Optional element to focus instead of the first focusable child. */
  initialFocus?: RefObject<HTMLElement | null>;
  children: ReactNode;
}

/**
 * Local dialog shell: overlay, Esc to close, initial focus, Tab wrap and focus
 * restoration to the element that opened the dialog (APP_FLOW §10).
 */
export function Modal({ labelledBy, onClose, initialFocus, children }: ModalProps) {
  const panelRef = useRef<HTMLDivElement>(null);
  const restoreRef = useRef<HTMLElement | null>(null);
  // Keep the latest callback without re-running the mount effect on every render.
  const onCloseRef = useRef(onClose);
  useEffect(() => {
    onCloseRef.current = onClose;
  }, [onClose]);

  const focusables = useCallback((): HTMLElement[] => {
    const panel = panelRef.current;
    if (!panel) return [];
    return Array.from(panel.querySelectorAll<HTMLElement>(FOCUSABLE)).filter(
      (node) => node.offsetParent !== null,
    );
  }, []);

  useEffect(() => {
    restoreRef.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    (initialFocus?.current ?? focusables()[0] ?? panelRef.current)?.focus();

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        onCloseRef.current();
        return;
      }
      if (event.key !== "Tab") return;
      const list = focusables();
      if (list.length === 0) {
        event.preventDefault();
        panelRef.current?.focus();
        return;
      }
      const first = list[0];
      const last = list[list.length - 1];
      if (!first || !last) return;
      const active = document.activeElement;
      if (event.shiftKey && (active === first || !panelRef.current?.contains(active))) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && active === last) {
        event.preventDefault();
        first.focus();
      }
    };

    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("keydown", onKeyDown);
      restoreRef.current?.focus();
    };
  }, [focusables, initialFocus]);

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-overlay/60 p-6"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div
        ref={panelRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby={labelledBy}
        tabIndex={-1}
        className="max-h-full w-[420px] overflow-y-auto rounded-3xl border border-hairline bg-canvas p-6"
      >
        {children}
      </div>
    </div>
  );
}
