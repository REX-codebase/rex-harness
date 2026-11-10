// What the composer says about web search. It used to print "REX-search not
// connected" whatever was true, including in the desktop app where REX-search
// is the built-in engine and on by default. The desktop runtime reports the
// provider that is actually active; with no backend there is nothing to report.
import { backendKind, listSearchProviders, type SearchProviderSummary } from "./backend";

export interface SearchStatus {
  label: string;
  title: string;
}

export const SEARCH_PREVIEW: SearchStatus = {
  label: "Search · preview",
  title: "No backend is connected, so web search is not available in this preview.",
};

export function searchStatusFor(providers: SearchProviderSummary[]): SearchStatus {
  const active = providers.find((p) => p.active);
  if (!active) return SEARCH_PREVIEW;
  return {
    label: `Search · ${active.name}`,
    title: active.built_in
      ? `${active.name} is the private built-in engine. Change it in Settings → Search provider.`
      : `${active.name} is the active search provider. Change it in Settings → Search provider.`,
  };
}

/** The live status, or the preview one when the desktop runtime is not there. */
export async function loadSearchStatus(): Promise<SearchStatus> {
  try {
    if ((await backendKind()) !== "tauri") return SEARCH_PREVIEW;
    return searchStatusFor(await listSearchProviders());
  } catch {
    return SEARCH_PREVIEW;
  }
}
