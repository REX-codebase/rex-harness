// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ProviderSummary, SignInState, SignInTicket } from "../data/backend";

const api = vi.hoisted(() => ({
  startChatGptSignIn: vi.fn(),
  pollChatGptSignIn: vi.fn(),
  cancelChatGptSignIn: vi.fn(),
  signOutChatGpt: vi.fn(),
}));

vi.mock("../data/backend", async () => ({
  ...(await vi.importActual<typeof import("../data/backend")>("../data/backend")),
  ...api,
}));

import { CHATGPT_USAGE_URL } from "../data/backend";
import { ChatGptSignIn, UsageLimitNotice, UsingChatGptPlan, isUsageLimit } from "./ChatGptPlan";

const TICKET: SignInTicket = {
  attempt: "att-1",
  launch_url: "http://127.0.0.1:1455/auth/start/abc",
  registering: true,
  timeout_secs: 300,
  opened: false,
};

function summary(over: Partial<ProviderSummary> & { status?: "supported" | "open-source-only" } = {}): ProviderSummary {
  const { status = "supported", ...rest } = over;
  return {
    id: "openai-chatgpt",
    name: "OpenAI (ChatGPT plan)",
    protocol: "open-ai-responses",
    discovery: "chat-gpt-plan-models",
    access: [
      {
        kind: "subscription-oauth",
        status,
        label: "Continue with ChatGPT",
        detail: "It opens when REX is released as open source.",
        sources: [],
        verified_on: "2026-11-09",
      },
    ],
    base_url: null,
    has_key: false,
    catalog: null,
    ...rest,
  };
}

const signedIn = (plan_usage: boolean): ProviderSummary =>
  summary({ has_key: plan_usage, signin: { signed_in: true, email: "alex@example.com", plan_usage, access_token_expires_at: 1 } });

async function tick(ms = 1100) {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(ms);
  });
}

beforeEach(() => {
  vi.useFakeTimers();
  window.localStorage.clear();
  for (const fn of Object.values(api)) fn.mockReset();
  api.cancelChatGptSignIn.mockResolvedValue(undefined);
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe("Continue with ChatGPT", () => {
  it("is shown but disabled, with the reason, while the route is closed", () => {
    render(<ChatGptSignIn summary={summary({ status: "open-source-only" })} onChange={() => {}} />);
    const button = screen.getByRole("button", { name: "Continue with ChatGPT" }) as HTMLButtonElement;
    expect(button.disabled).toBe(true);
    expect(screen.getByText(/released as open source/)).toBeTruthy();
    fireEvent.click(button);
    expect(api.startChatGptSignIn).not.toHaveBeenCalled();
  });

  it("sends nothing from a preview with no backend", () => {
    render(<ChatGptSignIn summary={undefined} onChange={() => {}} />);
    expect(screen.queryByRole("button")).toBeNull();
    expect(screen.getByText(/Nothing is sent from this preview/)).toBeTruthy();
  });

  it("starts the sign-in, offers the one-shot link, and can be cancelled", async () => {
    api.startChatGptSignIn.mockResolvedValue({ ok: true, ticket: TICKET });
    api.pollChatGptSignIn.mockResolvedValue({ state: "waiting" } satisfies SignInState);
    render(<ChatGptSignIn summary={summary()} onChange={() => {}} />);
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Continue with ChatGPT" }));
    });
    expect(api.startChatGptSignIn).toHaveBeenCalledWith(false, false);
    expect(screen.getByText(/Waiting for you to finish in the browser/)).toBeTruthy();
    expect(screen.getByText(/first sign-in here/)).toBeTruthy();
    const link = screen.getByRole("link", { name: "Open the sign-in page" }) as HTMLAnchorElement;
    expect(link.href).toBe(TICKET.launch_url);
    expect(link.rel).toContain("noopener");
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    });
    expect(api.cancelChatGptSignIn).toHaveBeenCalledWith("att-1");
    expect(screen.queryByText(/Waiting for you/)).toBeNull();
  });

  it("does not show the launch link when the backend already opened the browser", async () => {
    api.startChatGptSignIn.mockResolvedValue({ ok: true, ticket: { ...TICKET, opened: true } });
    api.pollChatGptSignIn.mockResolvedValue({ state: "waiting" });
    render(<ChatGptSignIn summary={summary()} onChange={() => {}} />);
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Continue with ChatGPT" }));
    });
    expect(screen.queryByRole("link", { name: "Open the sign-in page" })).toBeNull();
  });

  it("welcomes the person once after the first sign-in with plan usage on, and never again", async () => {
    const onChange = vi.fn();
    api.startChatGptSignIn.mockResolvedValue({ ok: true, ticket: TICKET });
    api.pollChatGptSignIn.mockResolvedValue({ state: "done", email: "alex@example.com", plan_usage: true });
    const { unmount } = render(<ChatGptSignIn summary={summary()} onChange={onChange} />);
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Continue with ChatGPT" }));
    });
    await tick();
    expect(onChange).toHaveBeenCalledWith("signed-in");
    expect(screen.getByRole("dialog").textContent).toContain("You're using your ChatGPT plan");
    const manage = screen.getByRole("link", { name: "Manage usage" }) as HTMLAnchorElement;
    expect(manage.href).toBe(CHATGPT_USAGE_URL);
    fireEvent.click(screen.getByRole("button", { name: "Got it" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    unmount();

    // a later sign-in (a fresh mount, as after a restart) stays quiet
    render(<ChatGptSignIn summary={summary()} onChange={onChange} />);
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Continue with ChatGPT" }));
    });
    await tick();
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(onChange).toHaveBeenCalledTimes(2);
  });

  it("says so, and shows no welcome, when plan usage was not allowed", async () => {
    api.startChatGptSignIn.mockResolvedValue({ ok: true, ticket: TICKET });
    api.pollChatGptSignIn.mockResolvedValue({ state: "done", email: null, plan_usage: false });
    render(<ChatGptSignIn summary={summary()} onChange={() => {}} />);
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Continue with ChatGPT" }));
    });
    await tick();
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.getByText(/plan usage was not allowed/)).toBeTruthy();
  });

  it("shows why a sign-in failed, in words from the backend", async () => {
    api.startChatGptSignIn.mockResolvedValue({ ok: true, ticket: TICKET });
    api.pollChatGptSignIn.mockResolvedValue({
      state: "failed",
      error: { kind: "credential", detail: "ChatGPT plan usage was declined; REX will not spend your plan until you allow it" },
    });
    render(<ChatGptSignIn summary={summary()} onChange={() => {}} />);
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Continue with ChatGPT" }));
    });
    await tick();
    expect(screen.getByRole("alert").textContent).toContain("was declined");
  });

  it("shows a refusal to start (the route is closed) verbatim", async () => {
    api.startChatGptSignIn.mockResolvedValue({
      ok: false,
      error: { kind: "credential", detail: "Using a ChatGPT plan is not available yet." },
    });
    render(<ChatGptSignIn summary={summary()} onChange={() => {}} />);
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Continue with ChatGPT" }));
    });
    expect(screen.getByRole("alert").textContent).toBe("Using a ChatGPT plan is not available yet.");
  });

  it("when signed in shows the account, Using ChatGPT plan with Manage usage, and signs out", async () => {
    const onChange = vi.fn();
    api.signOutChatGpt.mockResolvedValue({ ok: true, report: { revoked: true, note: null } });
    render(<ChatGptSignIn summary={signedIn(true)} onChange={onChange} />);
    expect(screen.getByText(/alex@example.com/)).toBeTruthy();
    expect(screen.getByText("Using ChatGPT plan")).toBeTruthy();
    expect((screen.getByRole("link", { name: "Manage usage" }) as HTMLAnchorElement).href).toBe(CHATGPT_USAGE_URL);
    expect(screen.queryByRole("button", { name: "Continue with ChatGPT" })).toBeNull();
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Sign out of ChatGPT" }));
    });
    expect(api.signOutChatGpt).toHaveBeenCalled();
    expect(onChange).toHaveBeenCalledWith("signed-out");
  });

  it("tells the person when revocation could not be confirmed", async () => {
    api.signOutChatGpt.mockResolvedValue({
      ok: true,
      report: { revoked: false, note: "REX signed you out here, but OpenAI did not confirm the session was revoked." },
    });
    render(<ChatGptSignIn summary={signedIn(true)} onChange={() => {}} />);
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Sign out of ChatGPT" }));
    });
    expect(screen.getByText(/did not confirm/)).toBeTruthy();
  });

  it("offers to allow plan usage again when it was declined, asking for consent anew", async () => {
    api.startChatGptSignIn.mockResolvedValue({ ok: true, ticket: TICKET });
    api.pollChatGptSignIn.mockResolvedValue({ state: "waiting" });
    render(<ChatGptSignIn summary={signedIn(false)} onChange={() => {}} />);
    expect(screen.getByText(/plan usage is not allowed/)).toBeTruthy();
    expect(screen.queryByText("Using ChatGPT plan")).toBeNull();
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Allow ChatGPT plan usage" }));
    });
    expect(api.startChatGptSignIn).toHaveBeenCalledWith(true, false);
  });

  it("offers a different account when one is remembered but signed out, and registers afresh", async () => {
    api.startChatGptSignIn.mockResolvedValue({ ok: true, ticket: TICKET });
    api.pollChatGptSignIn.mockResolvedValue({ state: "waiting" });
    const remembered = summary({
      signin: { signed_in: false, email: "alex@example.com", plan_usage: false, access_token_expires_at: null },
    });
    render(<ChatGptSignIn summary={remembered} onChange={() => {}} />);
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Use a different account" }));
    });
    expect(api.startChatGptSignIn).toHaveBeenCalledWith(false, true);
    // the ordinary button keeps the same account
    cleanup();
    api.startChatGptSignIn.mockClear();
    render(<ChatGptSignIn summary={remembered} onChange={() => {}} />);
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Continue with ChatGPT" }));
    });
    expect(api.startChatGptSignIn).toHaveBeenCalledWith(false, false);
  });

  it("does not offer a different account on a first sign-in or while signed in", () => {
    render(<ChatGptSignIn summary={summary()} onChange={() => {}} />);
    expect(screen.queryByRole("button", { name: "Use a different account" })).toBeNull();
    cleanup();
    render(<ChatGptSignIn summary={signedIn(true)} onChange={() => {}} />);
    expect(screen.queryByRole("button", { name: "Use a different account" })).toBeNull();
  });

  it("never renders anything credential-shaped", async () => {
    api.startChatGptSignIn.mockResolvedValue({ ok: true, ticket: TICKET });
    api.pollChatGptSignIn.mockResolvedValue({ state: "waiting" });
    const { container } = render(<ChatGptSignIn summary={summary()} onChange={() => {}} />);
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Continue with ChatGPT" }));
    });
    expect(container.innerHTML).not.toMatch(/id_token|access_token|refresh_token|Bearer|auth\.openai\.com/);
  });
});

describe("usage reminders", () => {
  it("Using ChatGPT plan links to usage settings", () => {
    render(<UsingChatGptPlan />);
    expect(screen.getByText("Using ChatGPT plan")).toBeTruthy();
    expect((screen.getByRole("link", { name: "Manage usage" }) as HTMLAnchorElement).href).toBe(CHATGPT_USAGE_URL);
  });

  it("the usage-limit state leads with Manage usage", () => {
    render(<UsageLimitNotice />);
    expect(screen.getByRole("alert").textContent).toContain("Usage limit reached");
    const primary = screen.getByRole("link", { name: "Manage usage" });
    expect(primary.className).toContain("plan-primary");
    expect(screen.queryByRole("button")).toBeNull();
  });

  it("recognises the usage-limit code in a run's error", () => {
    expect(isUsageLimit("provider HTTP 429: ChatGPT plan: usage limit reached [subscription_sharing_usage_limit_exceeded]")).toBe(true);
    expect(isUsageLimit("provider rate-limited the request")).toBe(false);
  });
});
