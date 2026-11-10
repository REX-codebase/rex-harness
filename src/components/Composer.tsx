import { Fragment, useEffect, useRef, useState, type CSSProperties } from "react";
import { ModelStatus } from "./ModelStatus";
import { EXAMPLE_TASKS, type RunState } from "../data/mock";
import type { ExecutionPath } from "../data/executionPath";
import { useSlideIndicator } from "./Fx";
import { defaultLean } from "../data/providers";
import { selectedModel } from "../data/custodyRun";
import { SEARCH_PREVIEW, loadSearchStatus, type SearchStatus } from "../data/searchStatus";
import { acknowledgeAutonomy, autonomyAcknowledged, autonomyAppliesTo } from "../data/autonomy";
import { AutonomyConfirm } from "./AutonomyConfirm";

// The headline arrives word by word; REX itself catches a single sheen.
const HEADLINE = ["Tell", "REX", "what", "to", "do."];

// Alt+P flips Build/Plan from inside the task field. Pure so it can be
// tested without a DOM: returns the next plan-mode value, or null when the
// key event is not the shortcut.
export function planShortcut(e: { altKey: boolean; ctrlKey: boolean; metaKey: boolean; shiftKey: boolean; key: string; code?: string }, planMode: boolean): boolean | null {
  if (!e.altKey || e.ctrlKey || e.metaKey || e.shiftKey) return null;
  // macOS Option+P yields "π" in e.key; e.code stays "KeyP".
  if (e.code === "KeyP" || e.key.toLowerCase() === "p") return !planMode;
  return null;
}

export function Composer({ task, setTask, state, onRun, planMode, setPlanMode, summarizeHistory = false, setSummarizeHistory, lean = null, setLean, fableGate, setFableGate, executionPath, locked = false, autonomous = false, setAutonomous }: { task: string; setTask: (t: string) => void; state: RunState; onRun: () => void; planMode: boolean; setPlanMode: (v: boolean) => void; summarizeHistory?: boolean; setSummarizeHistory?: (v: boolean) => void; lean?: boolean | null; setLean?: (v: boolean) => void; fableGate: boolean; setFableGate: (v: boolean) => void; executionPath: ExecutionPath; locked?: boolean; autonomous?: boolean; setAutonomous?: (v: boolean) => void }) {
  // `state` is the simulated tour's progress; `locked` is a real run (any
  // engine) that is still going. Either one holds the composer: a second run
  // started over a live one would orphan it.
  const busy = state === "working" || state === "verifying" || locked;
  const canRun = task.trim().length > 0 && !busy;
  const areaRef = useRef<HTMLTextAreaElement>(null);
  const optionsRef = useRef<HTMLDivElement>(null);
  const modeRef = useRef<HTMLDivElement>(null);
  // Build asks before each action, Plan asks once for the plan, Auto does not
  // ask. They exclude one another, so they are one control, not three switches.
  // Autonomy only counts where the run type honours it: if the person then
  // picks an installed agent, the screen must not keep claiming "autonomous".
  const autoAvailable = !!setAutonomous && autonomyAppliesTo(executionPath.kind);
  const runMode: "build" | "plan" | "auto" = autonomous && autoAvailable ? "auto" : planMode ? "plan" : "build";
  useSlideIndicator(modeRef, runMode, ".is-on");
  const [confirmingAuto, setConfirmingAuto] = useState(false);
  const chooseMode = (mode: "build" | "plan" | "auto") => {
    if (mode === "auto") {
      // The first time in a session asks; after that the choice is the switch.
      if (!autonomyAcknowledged()) {
        setConfirmingAuto(true);
        return;
      }
      setPlanMode(false);
      setAutonomous?.(true);
      return;
    }
    setAutonomous?.(false);
    setPlanMode(mode === "plan");
  };
  const [optionsOpen, setOptionsOpen] = useState(false);
  const [search, setSearch] = useState<SearchStatus>(SEARCH_PREVIEW);
  useEffect(() => {
    let active = true;
    void loadSearchStatus().then((status) => {
      if (active) setSearch(status);
    });
    return () => {
      active = false;
    };
  }, []);
  // Lean follows the selected provider until the person decides; the switch
  // shows what the next run will actually do.
  const leanOn = lean ?? defaultLean(selectedModel().provider);
  const extrasOn = (setSummarizeHistory && summarizeHistory ? 1 : 0) + (fableGate ? 1 : 0) + (setLean && leanOn ? 1 : 0);

  useEffect(() => {
    if (!optionsOpen) return;
    const onDown = (e: MouseEvent) => {
      if (optionsRef.current && !optionsRef.current.contains(e.target as Node)) setOptionsOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOptionsOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [optionsOpen]);

  return (
    <section aria-label="New task">
      <div className="mb-5 sm:mb-6">
        <p className="eyebrow">New task</p>
        <h1 className="mt-2 text-[30px] font-medium leading-[1.12] tracking-[-0.04em] text-text sm:text-[38px]">
          {HEADLINE.map((word, i) => (
            <Fragment key={i}>
              {i > 0 && " "}
              {/* The rise lives on the outer span and the skew on the inner one: word-rise animates transform, so a skew on the same element would snap when it ends. */}
              <span className="fx-word" style={{ "--w": i } as CSSProperties}>{word === "REX" ? <span className="rex-inline">REX</span> : word}</span>
            </Fragment>
          ))}
        </h1>
        <p className="mt-3 max-w-[54ch] text-sm leading-relaxed text-muted">One clear request. REX will work through it, verify the result, and leave a receipt.</p>
      </div>

      <div className={`task-field ${runMode === "plan" ? "is-plan" : ""} ${runMode === "auto" ? "is-auto" : ""} ${busy ? "is-busy" : ""}`} data-glow>
        <textarea
          id="task-input"
          ref={areaRef}
          rows={4}
          value={task}
          disabled={busy}
          onChange={(e) => setTask(e.target.value)}
          onKeyDown={(e) => {
            if ((e.metaKey || e.ctrlKey) && e.key === "Enter" && canRun) onRun();
            const next = planShortcut(e, planMode);
            if (next !== null) {
              e.preventDefault();
              chooseMode(next ? "plan" : "build");
            }
          }}
          aria-label="Task description"
          placeholder={runMode === "plan" ? "What should REX plan? Nothing runs until you approve." : runMode === "auto" ? "What should REX do on its own? It will not stop to ask." : "What needs to get done?"}
          spellCheck={false}
          className="task-input"
        />
        <div className="task-controls">
          <div className="flex min-w-0 items-center">
            <ModelStatus />
          </div>
          <div className="flex shrink-0 items-center gap-2">
            <div ref={modeRef} className="mode-seg" role="radiogroup" aria-label="Run mode" title="Alt+P switches between Build and Plan">
              <button type="button" role="radio" aria-checked={runMode === "build"} disabled={busy} onClick={() => chooseMode("build")} className={runMode === "build" ? "is-on" : ""} title="Build: REX asks before each file write and command">Build</button>
              <button type="button" role="radio" aria-checked={runMode === "plan"} disabled={busy} onClick={() => chooseMode("plan")} className={runMode === "plan" ? "is-on" : ""} title="Plan: REX proposes a plan and waits for your approval before anything runs">Plan</button>
              {setAutonomous && (
                <button
                  type="button"
                  role="radio"
                  aria-checked={runMode === "auto"}
                  disabled={busy || !autoAvailable}
                  onClick={() => chooseMode("auto")}
                  className={runMode === "auto" ? "is-on" : ""}
                  title={autoAvailable ? "Auto: REX works on its own without asking for approval, within its hard limits" : "Autonomous mode runs through the custody loop; this run type has its own gates"}
                >
                  Auto
                </button>
              )}
            </div>
            <div className="relative" ref={optionsRef}>
              <button type="button" className={`options-button ${optionsOpen ? "is-open" : ""}`} aria-haspopup="dialog" aria-expanded={optionsOpen} aria-label={`Run options${extrasOn ? `, ${extrasOn} on` : ""}`} onClick={() => setOptionsOpen((v) => !v)}>
                <svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true"><path d="M2.5 4.5h6m3 0h2m-11 7h2m3 0h6" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" /><circle cx="10" cy="4.5" r="1.6" fill="none" stroke="currentColor" strokeWidth="1.4" /><circle cx="6" cy="11.5" r="1.6" fill="none" stroke="currentColor" strokeWidth="1.4" /></svg>
                {extrasOn > 0 && <span className="options-count" aria-hidden="true">{extrasOn}</span>}
              </button>
              {optionsOpen && (
                <div className="options-menu" role="dialog" aria-label="Run options">
                  <p className="options-title">Run options</p>
                  {setSummarizeHistory && (
                    <label className="switch-row" title="Summarize old results: when older tool results no longer fit in REX's working memory, the model writes a short summary of them. Each summary is an extra model call on your key, so this is off by default.">
                      <span className="switch-copy"><b>Summarize old results</b><small>When old tool output no longer fits, the model writes a short summary. Costs an extra model call on your key.</small></span>
                      <input type="checkbox" role="switch" className="switch" checked={summarizeHistory} disabled={busy} onChange={(e) => setSummarizeHistory(e.target.checked)} aria-label="Summarize old results" />
                    </label>
                  )}
                  {setLean && (
                    <label className="switch-row" title="Lean prompt: sends about half as much to the model on every turn by offering 11 core tools, no skills catalog and compact state. It gives up web search and fetch, skills, memory and sub-agent exploration. On by default for free-tier providers, whose token limits are small.">
                      <span className="switch-copy"><b>Lean prompt</b><small>About half the tokens per turn, for free and small models. Drops web access, skills and memory.</small></span>
                      <input type="checkbox" role="switch" className="switch" checked={leanOn} disabled={busy} onChange={(e) => setLean(e.target.checked)} aria-label="Lean prompt" />
                    </label>
                  )}
                  <label className="switch-row" title="Fable gate: opens a THINK→PROVE→ATTACK→WRITE session with a mechanical deliberation timer and evidence-gated unlock. The countdown shows above the run.">
                    <span className="switch-copy"><b>Fable gate</b><small>Think, prove, attack, then write. A timer and evidence gate unlock each step.</small></span>
                    <input type="checkbox" role="switch" className="switch" checked={fableGate} disabled={busy} onChange={(e) => setFableGate(e.target.checked)} aria-label="Fable gate" />
                  </label>
                </div>
              )}
            </div>
            <button type="button" onClick={onRun} disabled={!canRun} title={!task.trim() ? "Describe a task first" : busy ? "A run is in progress" : `${runMode === "plan" ? "Draft the plan" : runMode === "auto" ? "Start the run; it will not ask for approval" : "Start the run"} (Ctrl+Enter)`} className="run-button" data-busy={busy ? "" : undefined} aria-label={busy ? "Run in progress" : runMode === "plan" ? "Plan task" : runMode === "auto" ? "Run task autonomously" : "Run task"}>
              <span>{busy ? "Running" : runMode === "plan" ? "Plan" : "Run"}</span>
              <svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true"><path d="M3.5 8h9m-3.5-3.5L12.5 8 9 11.5" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" /></svg>
            </button>
          </div>
        </div>
      </div>

      <div className="composer-meta">
        <span className="composer-meta-item" title={executionPath.detail}>
          <span className={`execution-path-dot execution-path-dot--${executionPath.kind}`} aria-hidden="true" />
          {executionPath.label}
        </span>
        {runMode === "plan" && <span className="composer-meta-item text-muted">Plan mode: nothing runs until you approve the plan</span>}
        {runMode === "auto" && <span className="composer-meta-item composer-meta-auto">Autonomous: no approval prompts. Cancel stops it.</span>}
        <span className="composer-meta-item" title={search.title}>{search.label}</span>
        <span className="composer-meta-keys hidden sm:inline-flex"><kbd>Ctrl</kbd><kbd>Enter</kbd> run<span className="mx-1.5 text-line">·</span><kbd>Alt</kbd><kbd>P</kbd> plan</span>
      </div>

      {confirmingAuto && (
        <AutonomyConfirm
          onCancel={() => setConfirmingAuto(false)}
          onConfirm={() => {
            acknowledgeAutonomy();
            setConfirmingAuto(false);
            setPlanMode(false);
            setAutonomous?.(true);
          }}
        />
      )}

      {state === "idle" && task.trim() === "" && (
        <button type="button" onClick={() => { setTask(EXAMPLE_TASKS[0]); areaRef.current?.focus(); }} className="example-prompt">
          <span className="text-faint">Try</span>
          <span className="truncate">{EXAMPLE_TASKS[0]}</span>
          <span aria-hidden="true">↗</span>
        </button>
      )}
    </section>
  );
}
