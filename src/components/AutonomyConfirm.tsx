import { useEffect, useRef } from "react";
import { AUTONOMY_TERMS } from "../data/autonomy";

// The one question autonomous mode asks before it switches on. It is a modal
// alert dialog: Cancel has focus and Escape (or a click outside) cancels, so a
// stray key never turns approvals off.
export function AutonomyConfirm({ onConfirm, onCancel }: { onConfirm: () => void; onCancel: () => void }) {
  const cancelRef = useRef<HTMLButtonElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    cancelRef.current?.focus();
  }, []);

  // Keep Tab inside the dialog while it is open.
  const onKeyDown = (event: React.KeyboardEvent) => {
    if (event.key === "Escape") {
      event.preventDefault();
      onCancel();
      return;
    }
    if (event.key !== "Tab") return;
    const focusable = panelRef.current?.querySelectorAll<HTMLElement>("button:not(:disabled)");
    if (!focusable || focusable.length === 0) return;
    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  };

  return (
    <div className="autonomy-overlay" onMouseDown={(e) => e.target === e.currentTarget && onCancel()} onKeyDown={onKeyDown}>
      <div
        ref={panelRef}
        className="autonomy-dialog"
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="autonomy-title"
        aria-describedby="autonomy-intro"
      >
        <p className="eyebrow">Autonomous mode</p>
        <h2 id="autonomy-title">Let REX work without asking?</h2>
        <p id="autonomy-intro" className="autonomy-intro">
          This turns off the approval prompt for each action. REX will keep going on its own until it finishes, hits a limit or you stop it.
        </p>
        <ul className="autonomy-terms">
          {AUTONOMY_TERMS.map((t) => (
            <li key={t.heading}>
              <b>{t.heading}</b>
              <span>{t.body}</span>
            </li>
          ))}
        </ul>
        <div className="autonomy-actions">
          <button ref={cancelRef} type="button" className="approval-deny" onClick={onCancel}>
            Keep asking me
          </button>
          <button type="button" className="approval-allow" onClick={onConfirm}>
            Turn on autonomous mode
          </button>
        </div>
      </div>
    </div>
  );
}
