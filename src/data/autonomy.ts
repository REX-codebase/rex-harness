// Autonomous mode: what it means, when it applies, and the one-time (per
// session) acknowledgement the person gives before it can be switched on.
//
// The backend decides what autonomy actually does (see `RunOptions::autonomous`
// in rex-providers). This file only states it in words the person reads before
// agreeing, so those words are kept next to a test that pins them.
import type { ExecutionPathKind } from "./executionPath";

/** What the person is agreeing to, in the order the dialog shows it. */
export const AUTONOMY_TERMS: { heading: string; body: string }[] = [
  {
    heading: "No approval prompts",
    body: "REX creates and edits files and runs commands in its own sandbox folder without stopping to ask. Every action is still listed in the run's activity, so you can read what it did.",
  },
  {
    heading: "The hard limits stay on",
    body: "Shells, interpreters, network tools and destructive programs stay blocked, a call outside the sandbox is still refused, and the run still stops at its step, token and time budgets.",
  },
  {
    heading: "You can stop it at any time",
    body: "Cancel ends the run immediately. Anything it changed is in its sandbox until you decide what to keep.",
  },
  {
    heading: "External tools still ask",
    body: "Calls into external MCP servers keep their approval prompt, because REX cannot see what those tools do.",
  },
];

/** Only the custody loop honours the switch; the other run types have their own gates. */
export function autonomyAppliesTo(kind: ExecutionPathKind): boolean {
  return kind === "custody";
}

const KEY = "rex-autonomy-acknowledged";
let acknowledged = false;

/**
 * Whether the person has already agreed to the terms in this session. It is
 * deliberately not kept across restarts: the mode removes a safeguard, so the
 * next session starts by asking again.
 */
export function autonomyAcknowledged(): boolean {
  if (acknowledged) return true;
  try {
    return window.sessionStorage.getItem(KEY) === "1";
  } catch {
    return false;
  }
}

export function acknowledgeAutonomy(): void {
  acknowledged = true;
  try {
    window.sessionStorage.setItem(KEY, "1");
  } catch {
    /* storage unavailable; the in-memory flag still holds for this page */
  }
}

/** Test hook: forget the acknowledgement. */
export function resetAutonomyAcknowledgement(): void {
  acknowledged = false;
  try {
    window.sessionStorage.removeItem(KEY);
  } catch {
    /* ignore */
  }
}
