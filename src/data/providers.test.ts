import { describe, expect, it } from "vitest";
import { PROVIDER_PRESETS, accessStatusFor, accessStatusLabel, defaultLean } from "./providers";

describe("provider presets", () => {
  it("ids are unique", () => {
    const ids = PROVIDER_PRESETS.map((p) => p.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("the free-tier presets are ordinary OpenAI-compatible API-key providers", () => {
    for (const id of ["nvidia", "groq", "openrouter"]) {
      const p = PROVIDER_PRESETS.find((x) => x.id === id)!;
      expect(p, id).toBeTruthy();
      expect(p.protocol).toBe("openai-compatible");
      expect(p.baseUrl).toMatch(/^https:\/\//);
      expect(p.modelDiscovery).toBe("openai-models");
      expect(p.access.map((a) => a.kind)).toEqual(["api-key"]);
      expect(p.note, `${id} should say something honest about its limits`).toBeTruthy();
    }
  });

  it("never claims a free tier can serve other people", () => {
    const text = PROVIDER_PRESETS.filter((p) => ["nvidia", "groq", "openrouter"].includes(p.id))
      .flatMap((p) => [p.note ?? "", ...p.access.map((a) => a.detail)])
      .join(" ")
      .toLowerCase();
    expect(text).not.toMatch(/unlimited|production|resell(?!ing)|serve your customers/);
  });

  it("the lean prompt is on by default only where tokens are scarce", () => {
    expect(defaultLean("groq")).toBe(true);
    expect(defaultLean("nvidia")).toBe(true);
    expect(defaultLean("openrouter")).toBe(true);
    for (const id of ["gemini", "anthropic", "openai", "local", "custom", "unknown-provider"]) {
      expect(defaultLean(id), id).toBe(false);
    }
  });

  it("tells local-server users a placeholder key is needed", () => {
    expect(PROVIDER_PRESETS.find((p) => p.id === "local")?.note).toMatch(/placeholder/i);
  });

  it("the ChatGPT plan is its own provider, signed in rather than pasted, and shown as opening with the release", () => {
    const p = PROVIDER_PRESETS.find((x) => x.id === "openai-chatgpt")!;
    expect(p.protocol).toBe("openai-responses");
    expect(p.modelDiscovery).toBe("chatgpt-plan-models");
    expect(p.access.map((a) => a.kind)).toEqual(["subscription-oauth"]);
    expect(p.access[0].status).toBe("open-source-only");
    expect(p.access[0].label).toBe("Continue with ChatGPT");
    // the API-key provider no longer claims anything about ChatGPT sign-in
    const api = PROVIDER_PRESETS.find((x) => x.id === "openai")!;
    expect(api.access.map((a) => a.kind)).toEqual(["api-key"]);
  });

  it("the backend's verdict wins over the bundled one, so a route that opens needs no frontend release", () => {
    const line = PROVIDER_PRESETS.find((x) => x.id === "openai-chatgpt")!.access[0];
    expect(accessStatusFor([], "openai-chatgpt", line)).toBe("open-source-only");
    const open = [{ id: "openai-chatgpt", access: [{ label: "Continue with ChatGPT", status: "supported" as const }] }];
    expect(accessStatusFor(open, "openai-chatgpt", line)).toBe("supported");
    expect(accessStatusFor(open, "gemini", line)).toBe("open-source-only");
  });

  it("every access status has a label", () => {
    for (const p of PROVIDER_PRESETS) for (const a of p.access) expect(accessStatusLabel[a.status], a.label).toBeTruthy();
  });
});
