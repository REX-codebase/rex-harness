import { useEffect, useRef, useState } from "react";
import { BackendBadge, TopBar } from "./components/TopBar";
import { Composer } from "./components/Composer";
import { AuraLayer, UltraTransition, type Origin } from "./components/Aura";
import { StateRail } from "./components/StateRail";
import { DesktopOnly, isDesktopRuntime } from "./components/DesktopOnly";
import { SessionView } from "./components/SessionView";
import { HistoryList } from "./components/HistoryList";
import { SettingsView } from "./components/SettingsView";
import {
  MOTION_KEY,
  loadMotionPref,
  resolveReduced,
  useSystemReducedMotion,
  type MotionPref,
} from "./data/motion";
import { BrowserView } from "./components/BrowserView";
import { PreviewRuntimeView } from "./components/PreviewRuntimeView";
import { NativePreviewView } from "./components/NativePreviewView";
import { ToolApprovalPreview } from "./components/ToolApprovalPreview";
import "./preview-runtime.css";
import { PAST_SESSIONS, newSession, startMockTurn, type Session } from "./data/mock";
import { AgentRunView } from "./components/AgentRunView";
import { UltraRunView } from "./components/UltraRunView";
import { ultraBegin, ultraSnapshot, ultraDecide, ultraCancel, type UltraSnapshot } from "./data/ultraRun";
import { liveRunAvailable } from "./data/liveRun";
import { custodyBegin, custodyDecidePlan, custodyStop, selectedModel } from "./data/custodyRun";
import { rexTaskBegin } from "./data/rexTasks";
import { FableCountdown } from "./components/FableCountdown";
import { loadFableSessionName, saveFableSessionName, fableCreateSession } from "./data/fable";
import { TerminalView } from "./components/TerminalView";
import { WorkspaceEditor } from "./components/WorkspaceEditor";
import { GitPanel } from "./components/GitPanel";
import { CheckpointPanel } from "./components/CheckpointPanel";
import { GitStatusChip, WorkspaceChip } from "./components/WorkspaceChip";
import { WorkspaceProvider } from "./data/workspaceContext";
import { RexTaskList, RexTaskView } from "./components/RexTaskView";
import {
  agentCancel,
  agentDecide,
  agentDecideAlways,
  canAllowForRun,
  agentSnapshot,
  agentTeardown,
  type AgentSnapshot,
} from "./data/agentRun";
import { terminalActive } from "./data/agentTypes";
import { InstalledAgentRunView } from "./components/InstalledAgentRunView";
import {
  installedAgentBegin,
  installedAgentCancel,
  installedAgentDecide,
  installedAgentSnapshot,
  installedRunTerminal,
  type InstalledRunSnapshot,
} from "./data/installedAgentRun";
import { loadInstalledAgentOptions } from "./components/ModelStatus";
import { OperatorModeChip, OperatorModeGate } from "./components/OperatorModeGate";
import { Ambient, runPhase } from "./components/Fx";
import { loadOperatorMode, saveOperatorMode, type OperatorMode } from "./data/operatorMode";
import { SetupChecklist } from "./components/SetupChecklist";
import type { InstalledAgentId } from "./data/backend";
import { resolveExecutionPath, type ExecutionPath } from "./data/executionPath";
import {
  FAILED_RUN_ID,
  LOST_CONTACT_AFTER,
  errorText,
  failedInstalledRun,
  failedRunSnapshot,
  runFinished,
  runInProgress,
} from "./data/runLifecycle";

const TOUR_KEY = "rex-tour-mode";

// Reads the installed-agent selection from settings. When an installed agent
// is selected it becomes the explicit execution path (see executionPath.ts);
// otherwise the custody loop is the default. Read at render for the path
// label and re-read inside onRun so the click always sees the latest choice.
export function readInstalledAgentSelection(): { id: InstalledAgentId; label: string } | null {
  try {
    const selected = JSON.parse(window.localStorage.getItem("rex-model-selection") || "null") as { provider?: string; id?: string; label?: string } | null;
    if (selected?.provider?.startsWith("installed:") && selected.id) {
      return { id: selected.id as InstalledAgentId, label: selected.label ?? selected.id };
    }
  } catch { /* no selection */ }
  return null;
}

// The hero starter composer exists only while there is no session. The moment
// the first run starts it settles - a brief blur + downward travel while its
// wrapper collapses - and unmounts, leaving exactly one composer at the
// bottom of the session. Reduced motion swaps it instantly.
// The folder the Terminal, Editor and Git tabs work in, and its git state, are
// shared by those tabs and the status chips, so they live above everything.
export default function App() {
  return (
    <WorkspaceProvider>
      <AppShell />
    </WorkspaceProvider>
  );
}

function AppShell() {
  const [operatorMode, setOperatorMode] = useState<OperatorMode | null>(() =>
    loadOperatorMode((k) => window.localStorage.getItem(k))
  );
  const chooseOperatorMode = (mode: OperatorMode) => {
    try { saveOperatorMode(mode); } catch { /* storage unavailable */ }
    setOperatorMode(mode);
  };
  const [view, setView] = useState<"task" | "settings" | "terminal" | "editor" | "git">("task");
  // The Terminal, Editor and Git tabs stay mounted once shown, so going to
  // another tab and back keeps your shell, your open file and its unsaved edits.
  const [visited, setVisited] = useState<ReadonlySet<string>>(() => new Set());
  useEffect(() => {
    setVisited((prev) => (prev.has(view) ? prev : new Set(prev).add(view)));
  }, [view]);
  const mounted = (tab: string) => visited.has(tab) || view === tab;
  const [browserPhase, setBrowserPhase] = useState<"closed" | "active" | "collapsed">("closed");
  const [browserRun, setBrowserRun] = useState(0);
  const [ultra, setUltra] = useState(false);
  const [ultraRun, setUltraRun] = useState<UltraSnapshot | null>(null);
  const ultraPoll = useRef<number | null>(null);
  const [ultraPulse, setUltraPulse] = useState(0);
  const [ultraOrigin, setUltraOrigin] = useState<Origin | null>(null);
  const [fast, setFast] = useState(false);
  const [fastPulse, setFastPulse] = useState(0);
  const [fastPhase, setFastPhase] = useState<"idle" | "engaging" | "disengaging">("idle");
  const fastTimer = useRef<number | null>(null);
  const [task, setTask] = useState("");
  const [session, setSession] = useState<Session | null>(null);
  // Active Fable gate session, if the user started one. The countdown is
  // visible here; the Composer toggle creates sessions.
  const [fableSession, setFableSession] = useState<string | null>(() => loadFableSessionName());
  // Fable gate toggle: when on, runs create a native fable session first.
  const [fableGate, setFableGate] = useState<boolean>(() => {
    try {
      return window.localStorage.getItem("rex-fable-gate") === "1";
    } catch {
      return false;
    }
  });
  const toggleFableGate = (v: boolean) => {
    try {
      window.localStorage.setItem("rex-fable-gate", v ? "1" : "0");
    } catch {
      /* storage unavailable */
    }
    setFableGate(v);
    if (!v) {
      // Turning the gate off clears the active session from the UI.
      // The session persists on disk; the countdown simply stops showing.
      saveFableSessionName(null);
      setFableSession(null);
    }
  };
  const [liveCapable, setLiveCapable] = useState(false);
  const [backendProbed, setBackendProbed] = useState(false);
  const [tourMode, setTourMode] = useState(() => {
    try {
      return window.localStorage.getItem(TOUR_KEY) === "1";
    } catch {
      return false;
    }
  });
  const [checklistHot, setChecklistHot] = useState(false);
  const checklistTimer = useRef<number | null>(null);
  const [liveRun, setLiveRun] = useState<AgentSnapshot | null>(null);
  const [custody, setCustody] = useState<{ grantId: string; phase: string; operator: string } | null>(null);
  const [rexTaskId, setRexTaskId] = useState<string | null>(null);
  const [liveStarting, setLiveStarting] = useState(false);
  const [liveDeciding, setLiveDeciding] = useState(false);
  const [liveCancelling, setLiveCancelling] = useState(false);
  // Plan mode: the run proposes a plan and parks in awaiting_plan until the
  // user approves it. Reset on each new run start.
  const [planMode, setPlanMode] = useState(false);
  // Autonomous mode: no approval prompts for the custody run. Unlike Plan it is
  // not reset by starting a run (a person running several tasks in a row should
  // not have to confirm each), but it is never remembered across sessions, and
  // the chip in the status row shows it whenever it is on.
  const [autonomous, setAutonomous] = useState(false);
  // Plan and autonomous mode exclude each other: a plan gate is a human
  // decision, and autonomous mode is the choice not to make them.
  const choosePlanMode = (on: boolean) => {
    setPlanMode(on);
    if (on) setAutonomous(false);
  };
  const chooseAutonomous = (on: boolean) => {
    setAutonomous(on);
    if (on) setPlanMode(false);
  };
  // Opt-in per run: each history summary is an extra model call.
  const [summarizeHistory, setSummarizeHistory] = useState(false);
  // Lean prompt: null follows the provider (on for the free-tier presets), an
  // explicit true/false is the person's own choice and is kept across runs.
  const [lean, setLean] = useState<boolean | null>(null);
  const [planDeciding, setPlanDeciding] = useState(false);
  const livePoll = useRef<number | null>(null);
  // Bumped whenever the screen lets go of its runs. A poll or a start that
  // answers after that belongs to a run the person already moved on from, and
  // is ignored instead of bringing the old run back.
  const runGen = useRef(0);
  // Set while a run is being started, before the render that disables Run, so
  // a second click or Ctrl+Enter in the same moment cannot start another run.
  const startGuard = useRef(false);
  // The backend stopped answering polls for a run that was still going.
  const [lostContact, setLostContact] = useState(false);
  const [installedRun, setInstalledRun] = useState<InstalledRunSnapshot | null>(null);
  const [installedStarting, setInstalledStarting] = useState(false);
  const [installedDeciding, setInstalledDeciding] = useState(false);
  const [installedCancelling, setInstalledCancelling] = useState(false);
  const installedPoll = useRef<number | null>(null);
  const [nativePreviewDir, setNativePreviewDir] = useState<string | null>(null);
  useEffect(() => {
    let active = true;
    liveRunAvailable()
      .then((ok) => {
        if (active) {
          setLiveCapable(ok);
          setBackendProbed(true);
        }
      })
      .catch(() => {
        if (active) setBackendProbed(true);
      });
    return () => { active = false; };
  }, []);
  // Re-probe on demand (the setup checklist's "check again" button): the
  // user may have started the sidecar after the first probe.
  const reprobeBackend = () => {
    liveRunAvailable()
      .then((ok) => {
        setLiveCapable(ok);
        setBackendProbed(true);
      })
      .catch(() => undefined);
  };
  const chooseTourMode = (on: boolean) => {
    setTourMode(on);
    try {
      if (on) window.localStorage.setItem(TOUR_KEY, "1");
      else window.localStorage.removeItem(TOUR_KEY);
    } catch {
      /* storage unavailable; tour mode just won't persist */
    }
  };
  // Deep link: #run=<id> reattaches to an existing agent loop run (e.g.
  // after a reload). Read-only viewers and the original driver share the
  // same snapshot stream.
  useEffect(() => {
    const m = window.location.hash.match(/^#run=(.+)$/);
    if (!m) return;
    const id = m[1];
    agentSnapshot(id)
      .then((snap) => {
        setLiveRun(snap);
        if (!terminalActive(snap)) startPolling(id);
      })
      .catch(() => undefined);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  const [hero, setHero] = useState<"shown" | "settling" | "gone">("shown");
  const cancel = useRef<(() => void) | null>(null);
  const settleTimer = useRef<number | null>(null);
  const systemReduce = useSystemReducedMotion();
  const [motion, setMotion] = useState<MotionPref>(() =>
    loadMotionPref((key) => window.localStorage.getItem(key))
  );
  const reduced = resolveReduced(motion, systemReduce);
  useEffect(() => {
    try {
      window.localStorage.setItem(MOTION_KEY, motion);
    } catch {
      /* storage unavailable; preference just won't persist */
    }
  }, [motion]);
  // Ultra owns the browser as a task tool, never as a destination. Turning
  // Ultra off removes the tool without changing the Standard task surface.
  useEffect(() => {
    if (!ultra) setBrowserPhase("closed");
  }, [ultra]);
  useEffect(
    () => () => {
      cancel.current?.();
      if (settleTimer.current !== null) window.clearTimeout(settleTimer.current);
      if (fastTimer.current !== null) window.clearTimeout(fastTimer.current);
      if (checklistTimer.current !== null) window.clearTimeout(checklistTimer.current);
      if (livePoll.current !== null) window.clearInterval(livePoll.current);
      if (ultraPoll.current !== null) window.clearInterval(ultraPoll.current);
      if (installedPoll.current !== null) window.clearInterval(installedPoll.current);
    },
    []
  );

  // One poll loop for every engine; the snapshot is the whole truth, so
  // rendering never depends on event timing. Each loop remembers which
  // generation of runs it was started for (see `runGen`) and goes quiet when
  // the screen has moved on. After LOST_CONTACT_AFTER failed polls in a row it
  // says so, instead of leaving a stale "Working" on screen forever.
  const follow = <T,>(
    slot: { current: number | null },
    fetchSnapshot: () => Promise<T>,
    apply: (snapshot: T) => void,
    finished: (snapshot: T) => boolean,
  ) => {
    if (slot.current !== null) window.clearInterval(slot.current);
    const gen = runGen.current;
    let misses = 0;
    const timer: number = window.setInterval(() => {
      fetchSnapshot()
        .then((snapshot) => {
          if (gen !== runGen.current) return;
          misses = 0;
          setLostContact(false);
          apply(snapshot);
          if (finished(snapshot)) {
            window.clearInterval(timer);
            if (slot.current === timer) slot.current = null;
          }
        })
        .catch(() => {
          if (gen !== runGen.current) return;
          misses += 1;
          if (misses >= LOST_CONTACT_AFTER) setLostContact(true);
        });
    }, 900);
    slot.current = timer;
  };
  const startInstalledPolling = (id: string) =>
    follow(installedPoll, () => installedAgentSnapshot(id), setInstalledRun, (snap) => snap.status !== "running");
  const startUltraPolling = (id: string) =>
    follow(ultraPoll, () => ultraSnapshot(id), setUltraRun, (snap) => Boolean(snap.terminal));
  const startPolling = (id: string) => follow(livePoll, () => agentSnapshot(id), setLiveRun, terminalActive);

  // Let go of every run on screen: stop following it and drop its state. Called
  // before a new run starts and by "New task", so a run from one engine can
  // never sit under the next run's view (an Ultra result used to stay above a
  // later custody run), and nothing is left polling.
  //
  // `release` also frees the finished run's live preview. Pass false to only
  // detach this screen ("Stop watching"): the backend's teardown cancels a run
  // that is still going, which is the opposite of what that button says.
  const clearRuns = (release = true) => {
    runGen.current += 1;
    for (const slot of [livePoll, ultraPoll, installedPoll]) {
      if (slot.current !== null) window.clearInterval(slot.current);
      slot.current = null;
    }
    if (release && liveRun && liveRun.id !== FAILED_RUN_ID) agentTeardown(liveRun.id).catch(() => undefined);
    setLostContact(false);
    setLiveRun(null);
    setCustody(null);
    setUltraRun(null);
    setInstalledRun(null);
    setRexTaskId(null);
    setNativePreviewDir(null);
    setLiveStarting(false);
    setInstalledStarting(false);
    setLiveDeciding(false);
    setLiveCancelling(false);
    setPlanDeciding(false);
    setInstalledDeciding(false);
    setInstalledCancelling(false);
  };

  const latest = session?.turns[session.turns.length - 1];
  const state = latest?.state ?? "idle";
  const busy = state === "working" || state === "verifying";

  const beginSettle = () => {
    setHero((prev) => (prev === "gone" ? prev : "settling"));
    if (settleTimer.current !== null) window.clearTimeout(settleTimer.current);
    settleTimer.current = window.setTimeout(() => setHero("gone"), reduced ? 40 : 520);
  };

  const toggleFast = () => {
    const next = !fast;
    if (fastTimer.current !== null) window.clearTimeout(fastTimer.current);
    setFastPhase(reduced ? "idle" : next ? "engaging" : "disengaging");
    setFast(next);
    setFastPulse((v) => v + 1);
    if (!reduced) fastTimer.current = window.setTimeout(() => setFastPhase("idle"), 1120);
  };

  const nativeProject = nativePreviewDir ?? new URLSearchParams(window.location.search).get("native-preview");
  // Whole words only: unanchored, "ui" matched "build" and "guide", and "app"
  // matched "mapping", so almost any task read as a web-page task.
  const previewTask = /\b(?:html|react|next\.?js|vite|astro|svelte|website|ui|interface|dashboard|landing page|app)\b/i.test(task);

  // One default execution path: the custody loop. Every alternative is an
  // explicit opt-in resolved here, so the Composer label and onRun below can
  // never disagree about what the Run button will do.
  const installedSelection = readInstalledAgentSelection();
  const executionPath: ExecutionPath = resolveExecutionPath({
    operatorMode,
    ultra,
    installedAgentId: installedSelection?.id ?? null,
    installedAgentLabel: installedSelection?.label ?? null,
    liveCapable,
    tourMode,
    autonomous,
  });

  // Everything that says "a run is going" or "a run just ended", from every
  // engine, in one place (see data/runLifecycle.ts).
  const runStates = {
    starting: liveStarting || installedStarting,
    agent: liveRun,
    ultra: ultraRun,
    installed: installedRun,
  };
  const runBusy = runInProgress(runStates);
  const runDone = runFinished(runStates);

  const onRun = async () => {
    cancel.current?.();
    const label = task.trim();
    // Run and Ctrl+Enter are disabled while a run is going; this also covers a
    // second press that lands before the screen has re-rendered. Starting over
    // a running run would orphan it: still spending budget, with an approval
    // nobody can see.
    if (!label || runBusy || startGuard.current) return;
    // Re-resolve at click time: the settings selection may have changed
    // since the last render.
    const selection = readInstalledAgentSelection();
    const path = resolveExecutionPath({
      operatorMode,
      ultra,
      installedAgentId: selection?.id ?? null,
      installedAgentLabel: selection?.label ?? null,
      liveCapable,
      tourMode,
      autonomous,
    });
    if (path.kind === "unavailable") {
      // No backend and no explicit tour: never silently animate a fake run.
      // Pulse the setup checklist instead — it is already rendered above. The
      // composer stays: settling it away here left nothing to type into.
      setChecklistHot(true);
      if (checklistTimer.current) window.clearTimeout(checklistTimer.current);
      checklistTimer.current = window.setTimeout(() => setChecklistHot(false), 1600);
      return;
    }
    if (path.kind === "tour") {
      const s = newSession(label);
      cancel.current = startMockTurn(s, label, "task", setSession, reduced);
      if (ultra) {
        setBrowserRun((v) => v + 1);
        setBrowserPhase("active");
      }
      beginSettle();
      return;
    }

    // Every live engine starts the same way: release whatever is on screen,
    // show that a start is under way, then ask its backend. The composer stays
    // where it is (locked while the run goes) because live runs have no
    // composer of their own below them.
    startGuard.current = true;
    clearRuns();
    const gen = runGen.current;
    const stillHere = () => gen === runGen.current;
    const { provider } = selectedModel();
    const installedBackend = path.kind === "installed" ? selection?.id : undefined;
    if (installedBackend) setInstalledStarting(true);
    else setLiveStarting(true);
    try {
      // Fable gate: when enabled, open a native fable session for this task
      // before the run starts. The session's authority timer and unlock rule
      // are enforced in Rust; the countdown appears above the run. If the
      // gate fails to open, the run does not start ungated — and says so
      // where the run would be (the setup checklist is not on screen when a
      // backend is connected, so pulsing it showed nothing).
      if (fableGate) {
        const sessionName = `fable-${Date.now().toString(36)}`;
        try {
          await fableCreateSession(sessionName, label);
          saveFableSessionName(sessionName);
          setFableSession(sessionName);
        } catch (e) {
          if (stillHere()) {
            setLiveRun(
              failedRunSnapshot(label, `the Fable gate could not open (${errorText(e)}), so the run was not started`, {
                provider,
                kind: "blocked",
              }),
            );
          }
          return;
        }
      }
      if (path.kind === "installed") {
        // Settings integration: the vendor CLI runs the task on an isolated
        // workspace while REX streams its events, applies the completion gate,
        // and holds any source changes for reviewed promotion.
        if (!installedBackend) return;
        const options = loadInstalledAgentOptions();
        try {
          const snap = await installedAgentBegin(installedBackend, label, options.workspace.trim());
          if (!stillHere()) return;
          setInstalledRun(snap);
          if (snap.status === "running") startInstalledPolling(snap.id);
        } catch (e) {
          if (stillHere()) setInstalledRun(failedInstalledRun(installedBackend, label, errorText(e)));
        }
        return;
      }
      try {
        if (path.kind === "ultra") {
          // Explicit verified-pipeline mode: contract -> build -> verify ->
          // adversary -> judge. A separate backend, chosen deliberately via the
          // Ultra toggle — not a silent branch.
          const snap = await ultraBegin(label);
          if (!stillHere()) return;
          setUltraRun(snap);
          startUltraPolling(snap.id);
        } else if (path.kind === "delegate") {
          // Explicit "delegate to external host": opens a durable REX task an
          // external host drives through rex-mcp; this UI supervises with the
          // permanent human Stop.
          const res = await rexTaskBegin(label, ultra);
          if (!res?.task_id) throw new Error("rex-mcp returned no task id");
          if (stillHere()) setRexTaskId(res.task_id);
        } else {
          // Default: the custody autonomous loop plans, acts through trusted
          // approvals, verifies its own work against gates, and stops
          // truthfully. Capture the plan-mode choice for this run, then reset
          // the checkbox so the next run starts in the default execution mode
          // unless the user opts in again.
          const mode = planMode;
          const summarize = summarizeHistory;
          setPlanMode(false);
          setSummarizeHistory(false);
          const view = await custodyBegin(label, mode, summarize, lean ?? undefined, autonomous);
          if (!stillHere()) return;
          setCustody({ grantId: view.grant_id, phase: view.phase, operator: view.operator });
          setLiveRun(view.snapshot);
          startPolling(view.snapshot.id);
        }
      } catch (e) {
        // The reason is shown where the run would have been, naming the
        // provider that was actually selected.
        if (stillHere()) setLiveRun(failedRunSnapshot(label, errorText(e), { provider }));
      }
    } finally {
      startGuard.current = false;
      if (stillHere()) {
        setLiveStarting(false);
        setInstalledStarting(false);
      }
    }
  };

  const onLiveDecision = (approved: boolean) => {
    if (ultraRun && !ultraRun.terminal) {
      if (liveDeciding) return;
      setLiveDeciding(true);
      ultraDecide(ultraRun.id, approved)
        .then((snap) => setUltraRun(snap))
        .catch(() => undefined)
        .finally(() => setLiveDeciding(false));
      return;
    }
    if (!liveRun || liveDeciding) return;
    setLiveDeciding(true);
    agentDecide(liveRun.id, approved)
      .then((snap) => setLiveRun(snap))
      .catch((e) => setLiveRun((prev) => (prev ? { ...prev, error: String(e) } : prev)))
      .finally(() => setLiveDeciding(false));
  };

  // "Allow for this run" on a pending command (desktop app only).
  const [allowForRunOk, setAllowForRunOk] = useState(false);
  useEffect(() => {
    canAllowForRun().then(setAllowForRunOk).catch(() => setAllowForRunOk(false));
  }, []);
  const onAllowForRun = () => {
    if (!liveRun || liveDeciding) return;
    setLiveDeciding(true);
    agentDecideAlways(liveRun.id)
      .then((snap) => setLiveRun(snap))
      .catch((e) => setLiveRun((prev) => (prev ? { ...prev, error: String(e) } : prev)))
      .finally(() => setLiveDeciding(false));
  };

  // Plan decision for a plan-gated custody run: approval resumes the run
  // into the normal loop; rejection ends it as denied. The decision travels
  // through the custody channel, which shares the same run registry, so the
  // gate's trusted-UI checks still apply.
  const onPlanDecision = (approved: boolean) => {
    if (!liveRun || planDeciding) return;
    setPlanDeciding(true);
    custodyDecidePlan(liveRun.id, approved)
      .then((snap) => setLiveRun(snap))
      .catch((e) => setLiveRun((prev) => (prev ? { ...prev, error: String(e) } : prev)))
      .finally(() => setPlanDeciding(false));
  };

  const onLiveCancel = () => {
    if (ultraRun && !ultraRun.terminal) {
      if (liveCancelling) return;
      setLiveCancelling(true);
      ultraCancel(ultraRun.id)
        .then((snap) => setUltraRun(snap))
        .catch(() => undefined)
        .finally(() => setLiveCancelling(false));
      return;
    }
    if (!liveRun || liveCancelling) return;
    if (custody) {
      // Custodied run: the Stop lands in custody first (terminal fence),
      // then the run loop is cancelled. Late success cannot win.
      setLiveCancelling(true);
      custodyStop(custody.grantId, liveRun.id)
        .then(() => agentSnapshot(liveRun.id))
        .then((snap) => {
          setLiveRun(snap);
          setCustody((prev) => (prev ? { ...prev, phase: "released" } : prev));
        })
        .catch(() => undefined)
        .finally(() => setLiveCancelling(false));
      return;
    }
    setLiveCancelling(true);
    agentCancel(liveRun.id)
      .then((snap) => setLiveRun(snap))
      .catch(() => undefined)
      .finally(() => setLiveCancelling(false));
  };

  const onInstalledDecision = (approved: boolean) => {
    if (!installedRun || installedDeciding) return;
    setInstalledDeciding(true);
    installedAgentDecide(installedRun.id, approved)
      .then((snap) => setInstalledRun(snap))
      .catch((e) => setInstalledRun((prev) => (prev ? { ...prev, error: String(e) } : prev)))
      .finally(() => setInstalledDeciding(false));
  };

  const onInstalledCancel = () => {
    if (!installedRun || installedCancelling) return;
    setInstalledCancelling(true);
    installedAgentCancel(installedRun.id)
      .then((snap) => setInstalledRun(snap))
      .catch(() => undefined)
      .finally(() => setInstalledCancelling(false));
  };

  const onFollowUp = (request: string) => {
    if (!session || busy) return;
    cancel.current?.();
    cancel.current = startMockTurn(session, request, "follow-up", setSession, reduced);
  };

  const onNewTask = () => {
    cancel.current?.();
    if (settleTimer.current !== null) window.clearTimeout(settleTimer.current);
    clearRuns();
    setSession(null);
    setTask("");
    setHero("shown");
    setBrowserPhase("closed");
    // The composer is the next thing the person wants; put the caret there.
    window.requestAnimationFrame(() => document.getElementById("task-input")?.focus());
  };

  const desktop = isDesktopRuntime();

  if (operatorMode === null) {
    return (
      <div className={`harness-shell min-h-full motion-${motion} ${reduced ? "reduced-fx" : ""}`} data-motion={motion} data-run="idle">
        <Ambient reduced={reduced} />
        <OperatorModeGate onChoose={chooseOperatorMode} />
      </div>
    );
  }

  // One word for what the run is doing, whichever engine drives it; the
  // backdrop tints and breathes to match. Presentation only.
  const phase = runPhase({
    agentStatus: liveRun?.status,
    installedStatus: installedRun?.status,
    ultraActive: !!ultraRun,
    ultraTerminalKind: ultraRun?.terminal?.kind,
    mockState: state,
  });

  return (
    <div className={`harness-shell min-h-full motion-${motion} ${ultra ? "ultra-on" : ""} ${fast ? "fast-on" : ""} ${fastPhase !== "idle" ? `fast-${fastPhase}` : ""} ${reduced ? "reduced-fx" : ""}`} data-motion={motion} data-ultra={ultra ? "on" : "off"} data-run={phase}>
      <Ambient reduced={reduced} />
      {ultraPulse > 0 && <UltraTransition key={ultraPulse} engaged={ultra} origin={ultraOrigin} />}
      {fastPhase !== "idle" && (
        <div className="fast-transition" key={fastPulse} aria-hidden="true">
          <div className="fast-iris"><i /><i /><i /></div>
          <div className="fast-rails"><i /><i /><i /><i /><i /><i /></div>
          <div className="fast-word"><span>FAST</span><small>LATENCY PROFILE / VISUAL PREVIEW</small></div>
          <div className="fast-cut fast-cut-a" /><div className="fast-cut fast-cut-b" />
        </div>
      )}
      <div className="ultra-atmosphere" aria-hidden="true"><span className="ultra-horizon" /><span className="ultra-scan" /></div>
      <AuraLayer />
      <TopBar view={view} onView={setView} ultra={ultra} onUltra={(origin) => { setUltraOrigin(origin); setUltra((v) => !v); setUltraPulse((v) => v + 1); }} fast={fast} onFast={toggleFast} />
      <div className="ultra-status" role="status" aria-live="polite"><span>ULTRA</span><b>{ultra ? "Verification tier engaged" : "Verification tier offline"}</b><small>{ultra ? "Contract, adversary and clean-room judge active" : "No extra capabilities are active"}</small></div>
      <div className="status-wrap mx-auto w-full max-w-[820px] px-5 pt-3 sm:px-8">
        <div className="status-row">
          <div className="status-group">
            <OperatorModeChip mode={operatorMode} onSwitch={chooseOperatorMode} />
            {autonomous && executionPath.kind === "custody" && (
              <span className="autonomy-chip" role="status" title="Runs start without approval prompts. Turn it off in the run mode control.">
                <i aria-hidden="true" />
                Autonomous · no approval prompts
              </span>
            )}
            <WorkspaceChip />
            <GitStatusChip onOpen={() => setView("git")} />
          </div>
          <BackendBadge live={liveCapable} />
        </div>
        {fableSession && <FableCountdown sessionName={fableSession} />}
      </div>
      <div className="fast-status" role="status" aria-live="polite"><span>FAST</span><b>{fast ? "TEMPO PROFILE ARMED" : "Fast preview off"}</b><small>Visual only · execution speed unchanged</small><i aria-hidden="true" /></div>
      <main className="main-spine mx-auto w-full max-w-[820px] px-5 pb-12 sm:px-8">
        {!desktop && (view === "terminal" || view === "editor" || view === "git") && <DesktopOnly feature={view} />}
        {desktop && mounted("terminal") && <TerminalView active={view === "terminal"} />}
        {desktop && mounted("editor") && <WorkspaceEditor active={view === "editor"} />}
        {desktop && mounted("git") && (
          <div className="git-view" hidden={view !== "git"}>
            <GitPanel />
            <CheckpointPanel />
          </div>
        )}
        {view === "settings" && (
          <SettingsView
            motion={motion}
            setMotion={setMotion}
            onReset={() => {
              try {
                window.localStorage.removeItem(MOTION_KEY);
              } catch {
                /* ignore */
              }
              setMotion("system");
            }}
          />
        )}
        {view === "task" && (
          <>
            {hero !== "gone" && (
              <div className={`hero-wrap ${hero === "settling" ? "settling" : ""}`}>
                <div>
                  <div
                    className="hero-settle-inner mb-9 mt-3 sm:mb-12 sm:mt-7"
                    aria-hidden={hero === "settling"}
                  >
                    <Composer
                      task={task}
                      setTask={setTask}
                      state={state}
                      onRun={onRun}
                      planMode={planMode}
                      setPlanMode={choosePlanMode}
                      autonomous={autonomous}
                      setAutonomous={chooseAutonomous}
                      summarizeHistory={summarizeHistory}
                      setSummarizeHistory={setSummarizeHistory}
                      lean={lean}
                      setLean={setLean}
                      fableGate={fableGate}
                      setFableGate={toggleFableGate}
                      executionPath={executionPath}
                      locked={runBusy}
                    />
                  </div>
                </div>
              </div>
            )}
            {/* Setup sits under the composer: the task box stays the first thing
                on screen, and step 3 ("describe a task above") reads true. */}
            {backendProbed && !liveCapable && (
              <SetupChecklist
                hot={checklistHot}
                tour={tourMode}
                onTour={chooseTourMode}
                onOpenSettings={() => setView("settings")}
                onRecheck={reprobeBackend}
              />
            )}
            <StateRail state={state} blockedReason={latest?.blockedReason} />
            {import.meta.env.DEV && new URLSearchParams(window.location.search).has("approval-preview") && <ToolApprovalPreview />}
            {(installedRun || installedStarting) && installedRun && (
              <InstalledAgentRunView
                run={installedRun}
                deciding={installedDeciding}
                cancelling={installedCancelling}
                onDecide={onInstalledDecision}
                onCancel={onInstalledCancel}
                onOpenPreview={(dir) => setNativePreviewDir(dir)}
              />
            )}
            {(installedRun || installedStarting) && !installedRun && (
              <section className="live-run" aria-label="Starting the installed agent">
                <div className="live-status" role="status">
                  <span className="live-dot" aria-hidden="true" />
                  <span className="eyebrow">Installed agent</span>
                  <span className="live-status-text">Starting the vendor CLI…</span>
                </div>
              </section>
            )}
            {(ultraRun || liveStarting) && ultraRun && (
              <UltraRunView
                run={ultraRun}
                deciding={liveDeciding}
                cancelling={liveCancelling}
                onDecide={onLiveDecision}
                onCancel={onLiveCancel}
              />
            )}
            {(liveRun || liveStarting) && liveRun && (
              <AgentRunView
                run={liveRun}
                deciding={liveDeciding}
                cancelling={liveCancelling}
                onDecide={onLiveDecision}
                onCancel={onLiveCancel}
                custody={custody}
                onPlanDecision={onPlanDecision}
                planDeciding={planDeciding}
                onAllowForRun={allowForRunOk ? onAllowForRun : undefined}
              />
            )}
            {lostContact && runBusy && (
              <p className="run-notice" role="alert">
                <span>Lost contact with the backend. The run may still be going; this screen cannot tell.</span>
                <button type="button" className="setup-link" onClick={() => clearRuns(false)}>
                  Stop watching
                </button>
              </p>
            )}
            {runDone && (
              <div className="run-next">
                <button type="button" className="newtask-button" onClick={onNewTask}>
                  <svg width="11" height="11" viewBox="0 0 12 12" aria-hidden="true"><path d="M6 1.5v9M1.5 6h9" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" /></svg>
                  New task
                </button>
              </div>
            )}
            {rexTaskId && (
              <RexTaskView taskId={rexTaskId} onClose={() => setRexTaskId(null)} />
            )}
            {!rexTaskId && operatorMode === "agent" && !liveRun && !liveStarting && (
              <RexTaskList onPick={setRexTaskId} />
            )}
            {(liveRun || liveStarting) && !liveRun && (
              <section className="live-run" aria-label="Starting the agent loop">
                <div className="live-status" role="status">
                  <span className="live-dot" aria-hidden="true" />
                  <span className="eyebrow">Agent loop</span>
                  <span className="live-status-text">Contacting the live model catalog…</span>
                </div>
              </section>
            )}
            {!liveRun && !liveStarting && (nativeProject ? <NativePreviewView projectDir={nativeProject} /> : session && previewTask && <PreviewRuntimeView />)}
            {ultra && session && !previewTask && browserPhase === "active" && (
              <BrowserView
                key={browserRun}
                reduced={reduced}
                onFinished={() => setBrowserPhase("collapsed")}
              />
            )}
            {ultra && session && browserPhase === "collapsed" && (
              <button
                type="button"
                className="browser-collapsed"
                onClick={() => { setBrowserRun((v) => v + 1); setBrowserPhase("active"); }}
                aria-label="Reopen the simulated browser work receipt"
              >
                <span><b>Browser work complete</b><small>Tool closed · receipt B-0119 · SAMPLE</small></span>
                <span>Review</span>
              </button>
            )}
            {!liveRun && !liveStarting && !ultraRun && !installedRun && !installedStarting && !rexTaskId && (session ? (
              <SessionView
                key={session.id}
                session={session}
                busy={busy}
                onFollowUp={onFollowUp}
                onNewTask={onNewTask}
                focusComposer={session.id === "run-live"}
              />
            ) : (
              <div className="empty-state">
                <span className="empty-mark" />
                <p>Receipts and checks will appear here after a run.</p>
              </div>
            ))}
            {/* Sample history belongs to the no-backend preview. With a backend
                connected it listed invented runs ("Publish the billing webhook
                to production") under the person's real one. */}
            {backendProbed && !liveCapable && (
              <HistoryList
                sessions={PAST_SESSIONS}
                selectedId={session?.id}
                onSelect={(s) => {
                  cancel.current?.();
                  setSession(s);
                  beginSettle();
                }}
              />
            )}
          </>
        )}
      </main>
    </div>
  );
}
