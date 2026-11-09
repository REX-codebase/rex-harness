import { useCallback, useEffect, useRef, useState } from "react";
import {
  CHATGPT_USAGE_URL,
  cancelChatGptSignIn,
  describeError,
  pollChatGptSignIn,
  signOutChatGpt,
  startChatGptSignIn,
  type ProviderSummary,
  type SignInTicket,
} from "../data/backend";
import "../chatgpt.css";

// Everything the person sees about spending their ChatGPT plan in REX, in the
// shapes OpenAI's UI/UX guidelines ask for (developers.openai.com/siwc/ui-ux-guidelines):
//  - "Continue with ChatGPT" starts the sign-in;
//  - a welcome modal, once, after the first sign-in with plan usage on;
//  - "Using ChatGPT plan" with a "Manage usage" link wherever the plan is in use;
//  - a usage-limit state whose primary action is "Manage usage".
// TODO(release): use OpenAI's approved ChatGPT logo and button asset
// (developers.openai.com/siwc/website) in ChatGptButton once the assets are
// licensed for REX; the label and behaviour here are already the approved ones.

const WELCOMED_KEY = "rex-chatgpt-welcomed";

function alreadyWelcomed(): boolean {
  try {
    return window.localStorage.getItem(WELCOMED_KEY) === "1";
  } catch {
    return false;
  }
}

function markWelcomed() {
  try {
    window.localStorage.setItem(WELCOMED_KEY, "1");
  } catch {
    /* a private window forgets; the modal may show once more, which is harmless */
  }
}

const POLL_MS = 1000;

/** The error text a run ends with when ChatGPT plan usage is used up. */
export function isUsageLimit(detail: string): boolean {
  return detail.includes("subscription_sharing_usage_limit_exceeded");
}

export function ManageUsageLink({ className }: { className?: string }) {
  return (
    <a className={className ?? "plan-manage"} href={CHATGPT_USAGE_URL} target="_blank" rel="noopener noreferrer">
      Manage usage
    </a>
  );
}

/** Shown near the model selector whenever requests spend the ChatGPT plan. */
export function UsingChatGptPlan() {
  return (
    <p className="plan-badge" role="note">
      <b>Using ChatGPT plan</b>
      <ManageUsageLink />
    </p>
  );
}

/** A run stopped because the plan's (or this app's) usage limit was reached. */
export function UsageLimitNotice() {
  return (
    <div className="usage-limit" role="alert">
      <p className="plan-badge">
        <b>ChatGPT plan</b>
      </p>
      <h3>Usage limit reached</h3>
      <p>
        Your ChatGPT plan, or the limit you set for REX, has no usage left right now. Review the limit in ChatGPT settings.
      </p>
      <ManageUsageLink className="plan-primary" />
    </div>
  );
}

export function ChatGptWelcome({ onClose }: { onClose: () => void }) {
  return (
    <div className="connect-backdrop" role="presentation">
      <section role="dialog" aria-modal="true" aria-labelledby="chatgpt-welcome-title" className="connect-dialog">
        <p className="eyebrow">ChatGPT</p>
        <h2 id="chatgpt-welcome-title">You&apos;re using your ChatGPT plan</h2>
        <p className="dialog-copy">
          Eligible AI requests in REX now use your ChatGPT plan. You can review and limit that usage in ChatGPT settings at any time.
        </p>
        <p className="dialog-copy">
          <ManageUsageLink />
        </p>
        <button type="button" className="dialog-primary is-active" autoFocus onClick={onClose}>
          Got it
        </button>
      </section>
    </div>
  );
}

export function ChatGptButton({ onClick, disabled, children }: { onClick: () => void; disabled?: boolean; children?: string }) {
  return (
    <button type="button" className="chatgpt-button" disabled={disabled} onClick={onClick}>
      {children ?? "Continue with ChatGPT"}
    </button>
  );
}

type Phase =
  | { kind: "idle" }
  | { kind: "starting" }
  | { kind: "waiting"; ticket: SignInTicket }
  | { kind: "error"; message: string };

/**
 * The ChatGPT sign-in for one place in the UI (the settings card and the
 * connect dialog). `summary` is the backend's view of the provider; the
 * backend, not this component, decides whether the route is open.
 */
export function ChatGptSignIn({
  summary,
  onChange,
}: {
  summary: ProviderSummary | undefined;
  /** The signed-in state changed: refresh summaries and the model list. */
  onChange: (change: "signed-in" | "signed-out") => void;
}) {
  const [phase, setPhase] = useState<Phase>({ kind: "idle" });
  const [welcome, setWelcome] = useState(false);
  const [note, setNote] = useState<string | null>(null);
  const timer = useRef<number | null>(null);
  const alive = useRef(true);

  const line = summary?.access?.find((a) => a.kind === "subscription-oauth");
  const open = line?.status === "supported";
  const signin = summary?.signin;

  const stopPolling = useCallback(() => {
    if (timer.current !== null) {
      window.clearInterval(timer.current);
      timer.current = null;
    }
  }, []);

  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
      stopPolling();
    };
  }, [stopPolling]);

  const watch = useCallback(
    (ticket: SignInTicket) => {
      stopPolling();
      timer.current = window.setInterval(async () => {
        const state = await pollChatGptSignIn(ticket.attempt);
        if (!alive.current || !state || state.state === "waiting") return;
        stopPolling();
        if (state.state === "done") {
          setPhase({ kind: "idle" });
          if (state.plan_usage) {
            if (!alreadyWelcomed()) setWelcome(true);
          } else {
            setNote("Signed in, but ChatGPT plan usage was not allowed, so REX cannot spend your plan yet.");
          }
          onChange("signed-in");
        } else if (state.state === "failed") {
          setPhase({ kind: "error", message: describeError(state.error) });
        } else {
          setPhase({ kind: "idle" });
        }
      }, POLL_MS);
    },
    [onChange, stopPolling]
  );

  const start = async (forceConsent: boolean) => {
    setNote(null);
    setPhase({ kind: "starting" });
    const started = await startChatGptSignIn(forceConsent);
    if (!alive.current) return;
    if (!started.ok) {
      setPhase({ kind: "error", message: describeError(started.error) });
      return;
    }
    setPhase({ kind: "waiting", ticket: started.ticket });
    watch(started.ticket);
  };

  const cancel = async () => {
    if (phase.kind !== "waiting") return;
    stopPolling();
    await cancelChatGptSignIn(phase.ticket.attempt).catch(() => undefined);
    if (alive.current) setPhase({ kind: "idle" });
  };

  const signOut = async () => {
    setNote(null);
    const result = await signOutChatGpt();
    if (!alive.current) return;
    if (!result.ok) {
      setPhase({ kind: "error", message: describeError(result.error) });
      return;
    }
    if (result.report.note) setNote(result.report.note);
    onChange("signed-out");
  };

  const closeWelcome = () => {
    markWelcomed();
    setWelcome(false);
  };

  const busy = phase.kind === "starting" || phase.kind === "waiting";

  return (
    <div className="chatgpt-signin" aria-label="Use your ChatGPT plan">
      {welcome && <ChatGptWelcome onClose={closeWelcome} />}
      <h3>Use your ChatGPT plan</h3>
      <p className="dialog-copy">
        Complete eligible AI requests in REX with usage included in your ChatGPT plan or credits balance. This is separate from any API
        billing.
      </p>

      {!summary && <p className="plan-state">Needs the desktop app or the dev sidecar. Nothing is sent from this preview.</p>}

      {summary && !open && (
        <>
          <ChatGptButton onClick={() => undefined} disabled />
          <p className="plan-state">{line?.detail ?? "This route is not open yet."}</p>
        </>
      )}

      {summary && open && signin?.signed_in && signin.plan_usage && (
        <>
          <p className="plan-state">
            Signed in{signin.email ? ` as ${signin.email}` : ""}.
          </p>
          <UsingChatGptPlan />
          <button type="button" className="plan-secondary" onClick={signOut}>
            Sign out of ChatGPT
          </button>
        </>
      )}

      {summary && open && signin?.signed_in && !signin.plan_usage && (
        <>
          <p className="plan-state">
            Signed in{signin.email ? ` as ${signin.email}` : ""}, but ChatGPT plan usage is not allowed for REX, so it cannot spend your plan.
          </p>
          <ChatGptButton onClick={() => start(true)} disabled={busy}>
            Allow ChatGPT plan usage
          </ChatGptButton>
          <button type="button" className="plan-secondary" onClick={signOut}>
            Sign out of ChatGPT
          </button>
        </>
      )}

      {summary && open && !signin?.signed_in && (
        <>
          <ChatGptButton onClick={() => start(false)} disabled={busy} />
          {phase.kind === "waiting" && (
            <div className="plan-state" role="status">
              <p>
                Waiting for you to finish in the browser…
                {phase.ticket.registering && " This is your first sign-in here: OpenAI will ask you to approve REX."}
              </p>
              {!phase.ticket.opened && (
                <p>
                  <a href={phase.ticket.launch_url} target="_blank" rel="noopener noreferrer">
                    Open the sign-in page
                  </a>{" "}
                  <small>(works once, on this computer)</small>
                </p>
              )}
              <button type="button" className="plan-secondary" onClick={cancel}>
                Cancel
              </button>
            </div>
          )}
        </>
      )}

      {phase.kind === "error" && (
        <p className="provider-err-line" role="alert">
          {phase.message}
        </p>
      )}
      {note && (
        <p className="plan-state" role="status">
          {note}
        </p>
      )}
    </div>
  );
}
