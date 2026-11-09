import type { RunState } from "../data/mock";

const STAGES: { key: RunState; label: string }[] = [
  { key: "working", label: "Working" },
  { key: "verifying", label: "Verifying" },
  { key: "done", label: "Done" },
];

type StageStatus = "pending" | "active" | "complete";

function stage(stageKey: RunState, state: RunState): StageStatus {
  if (state === "idle" || state === "blocked") return "pending";
  if (state === "done") return "complete";
  const order: RunState[] = ["working", "verifying", "done"];
  const a = order.indexOf(stageKey);
  const b = order.indexOf(state);
  return a < b ? "complete" : a === b ? "active" : "pending";
}

// The run's state as a rail of nodes. Activity is shown, never progress: an
// active stage lights and its outgoing link carries a travelling pulse, but
// no link fills until the stage after it is genuinely reached.
export function StateRail({ state, blockedReason }: { state: RunState; blockedReason?: string }) {
  if (state === "idle") return null;
  if (state === "blocked") {
    return (
      <div aria-live="polite" className="state-row">
        <div className="flex items-start gap-3">
          <span className="blocked-dot mt-1.5 h-2 w-2 shrink-0 rounded-full bg-blocked" />
          <div>
            <p className="text-sm font-medium text-text">Blocked</p>
            <p className="mt-1 text-sm leading-relaxed text-muted">{blockedReason ?? "The agent stopped and explained why."}</p>
          </div>
        </div>
      </div>
    );
  }
  return (
    <div aria-live="polite" className="state-row" data-state={state}>
      <ol className="rail">
        {STAGES.map((s, i) => {
          const st = stage(s.key, state);
          return (
            <li key={s.key} className="rail-stage" data-st={st}>
              <span className="rail-node" aria-hidden="true">
                <svg className="rail-check" viewBox="0 0 16 16"><path d="M4.6 8.3 7 10.7 11.4 5.6" /></svg>
              </span>
              <span
                className={`rail-label ${st === "active" ? "text-text" : st === "complete" ? "text-muted" : "text-faint"}`}
                aria-current={st === "active" ? "step" : undefined}
              >
                {s.label}
              </span>
              {i < STAGES.length - 1 && (
                <span className="rail-link" data-st={st} aria-hidden="true"><i /></span>
              )}
            </li>
          );
        })}
      </ol>
    </div>
  );
}
