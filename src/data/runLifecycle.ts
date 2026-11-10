// Run lifecycle rules shared by the task screen: when a run is still going,
// what a run that could not start looks like, and how to word an error.
//
// These are pure so the rules can be tested without rendering the app. They
// exist because the screen used to decide each of these separately for every
// engine (custody, Ultra, installed agent, delegate), and the copies drifted:
// a second run could be started over a running one, and a failed start always
// claimed the provider was "gemini".
import { terminalActive, type AgentSnapshot, type TerminalReason } from "./agentTypes";
import type { InstalledRunSnapshot } from "./installedAgentRun";
import type { UltraSnapshot } from "./ultraRun";

/** Id of the stand-in run shown when a run could not even be started. */
export const FAILED_RUN_ID = "run-failed";

/** A readable message from whatever a bridge call rejected with. */
export function errorText(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  try {
    return JSON.stringify(error) ?? String(error);
  } catch {
    return String(error);
  }
}

/**
 * The run record shown when a run never started, so the reason sits where the
 * run would have been. `provider` is the one the person selected, not a
 * default. A refusal by REX itself (for example a gate that would not open)
 * is `blocked`; a failed call to the provider or backend is `provider_error`.
 */
export function failedRunSnapshot(
  task: string,
  detail: string,
  options: { provider: string; kind?: "provider_error" | "blocked" },
): AgentSnapshot {
  const blocked = options.kind === "blocked";
  const terminal_reason: TerminalReason = blocked
    ? { kind: "blocked", detail }
    : { kind: "provider_error", detail };
  return {
    id: FAILED_RUN_ID,
    task,
    status: blocked ? "blocked" : "failed",
    terminal_reason,
    provider: options.provider,
    model: "",
    plan: [],
    step: 0,
    max_steps: 0,
    tool_calls: 0,
    max_tool_calls: 0,
    tokens_used: 0,
    max_tokens: 0,
    elapsed_ms: 0,
    max_wall_ms: 0,
    pending_approval: null,
    events: [],
    preview: null,
    completion_summary: null,
    error: detail,
  };
}

/** The same stand-in for an installed-agent run that could not be started. */
export function failedInstalledRun(
  backend: InstalledRunSnapshot["backend"],
  prompt: string,
  detail: string,
  now: number = Date.now(),
): InstalledRunSnapshot {
  return {
    id: FAILED_RUN_ID,
    backend,
    status: "failed",
    prompt,
    workspace: "",
    staging_workspace: "",
    preview_dir: "",
    model: null,
    effort: null,
    created_at_ms: now,
    updated_at_ms: now,
    exit_code: null,
    events: [],
    stderr_tail: "",
    diff: null,
    promotion: "not_required",
    completion: null,
    error: detail,
  };
}

export interface RunStates {
  /** A run is being started (the bridge call has not answered yet). */
  starting: boolean;
  /** The custody / delegate run, if any. */
  agent: AgentSnapshot | null;
  ultra: UltraSnapshot | null;
  installed: InstalledRunSnapshot | null;
}

/**
 * True while any engine still has a run that needs to finish or be decided.
 * Starting another then would orphan it: the screen would stop following it
 * while it kept spending budget and waiting on an approval nobody can see.
 * A staged installed-agent result counts, because promoting or discarding it
 * is still the person's decision.
 */
export function runInProgress(state: RunStates): boolean {
  if (state.starting) return true;
  if (state.agent && !terminalActive(state.agent)) return true;
  if (state.ultra && !state.ultra.terminal) return true;
  if (state.installed && (state.installed.status === "running" || state.installed.status === "awaiting_review")) {
    return true;
  }
  return false;
}

/** A run has ended and its result is still on screen. */
export function runFinished(state: RunStates): boolean {
  return (state.agent !== null || state.ultra !== null || state.installed !== null) && !runInProgress(state);
}

/** Consecutive failed polls after which the screen says it lost the backend. */
export const LOST_CONTACT_AFTER = 8;
