import { describe, expect, it } from "vitest";
import { SEARCH_PREVIEW, searchStatusFor } from "./searchStatus";
import type { SearchProviderSummary } from "./backend";

const provider = (p: Partial<SearchProviderSummary>): SearchProviderSummary => ({
  id: "rex",
  name: "REX-search",
  active: false,
  built_in: true,
  has_key: true,
  endpoint: "",
  docs_url: "",
  ...p,
});

describe("search status", () => {
  it("names the provider that is actually active", () => {
    const status = searchStatusFor([provider({}), provider({ id: "exa", name: "Exa", built_in: false, active: true })]);
    expect(status.label).toBe("Search · Exa");
    expect(status.title).toContain("active search provider");
  });

  it("calls the built-in engine private and built in, not 'not connected'", () => {
    const status = searchStatusFor([provider({ active: true })]);
    expect(status.label).toBe("Search · REX-search");
    expect(status.label).not.toMatch(/not connected/i);
    expect(status.title).toContain("built-in");
  });

  it("falls back to the preview wording when nothing reports as active", () => {
    expect(searchStatusFor([])).toBe(SEARCH_PREVIEW);
    expect(searchStatusFor([provider({ active: false })])).toBe(SEARCH_PREVIEW);
  });
});
