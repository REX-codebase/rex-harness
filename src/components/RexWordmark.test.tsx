// @vitest-environment jsdom
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { BrandName, FIRST_TYPE_DELAY_MS, RETYPE_DELAY_MS, RexWordmark, TYPE_STEP_MS, WORDMARK } from "./RexWordmark";
import { UltraTransition } from "./Aura";

afterEach(cleanup);

const glyphRun = (c: HTMLElement) => c.querySelector<HTMLElement>(".rex-glyphs")!;

describe("RexWordmark", () => {
  it("is live text, announced once as REX, not an image", () => {
    const { container } = render(<RexWordmark />);
    expect(screen.getByRole("img", { name: "REX" })).toBeTruthy();
    expect(container.querySelector("img")).toBeNull();
    expect(Array.from(container.querySelectorAll(".rex-glyph")).map((g) => g.textContent).join("")).toBe(WORDMARK);
  });

  it("hides the decorative layers from assistive tech", () => {
    const { container } = render(<RexWordmark />);
    expect(glyphRun(container).getAttribute("aria-hidden")).toBe("true");
    expect(container.querySelector(".rex-halo")?.getAttribute("aria-hidden")).toBe("true");
  });

  it("types each glyph in turn, with the caret on the last one", () => {
    const { container } = render(<RexWordmark />);
    const glyphs = Array.from(container.querySelectorAll<HTMLElement>(".rex-glyph"));
    expect(glyphs.map((g) => g.style.getPropertyValue("--i"))).toEqual(["0", "1", "2"]);
    expect(glyphs.map((g) => g.classList.contains("is-last"))).toEqual([false, false, true]);
    expect(glyphRun(container).style.getPropertyValue("--type-delay")).toBe(`${FIRST_TYPE_DELAY_MS}ms`);
    expect(glyphRun(container).style.getPropertyValue("--type-step")).toBe(`${TYPE_STEP_MS}ms`);
  });

  it("retypes on an Ultra change, later, so it lands as the transition clears", () => {
    const { container, rerender } = render(<RexWordmark ultra={false} />);
    const first = glyphRun(container);
    rerender(<RexWordmark ultra />);
    const second = glyphRun(container);
    expect(second).not.toBe(first); // keyed: a fresh run replays the typing
    expect(second.style.getPropertyValue("--type-delay")).toBe(`${RETYPE_DELAY_MS}ms`);
    expect(container.querySelector(".rex-wordmark")?.classList.contains("is-ultra")).toBe(true);
  });

  it("does not replay or retime the typing on unrelated re-renders", () => {
    // Changing animation-delay under a finished animation would snap the
    // letters back to their hidden first frame.
    const { container, rerender } = render(<RexWordmark ultra={false} />);
    const first = glyphRun(container);
    rerender(<RexWordmark ultra={false} />);
    rerender(<RexWordmark ultra={false} />);
    expect(glyphRun(container)).toBe(first);
    expect(first.style.getPropertyValue("--type-delay")).toBe(`${FIRST_TYPE_DELAY_MS}ms`);
    rerender(<RexWordmark ultra />);
    const ultraRun = glyphRun(container);
    rerender(<RexWordmark ultra />);
    expect(glyphRun(container)).toBe(ultraRun);
    expect(ultraRun.style.getPropertyValue("--type-delay")).toBe(`${RETYPE_DELAY_MS}ms`);
  });
});

describe("BrandName", () => {
  it("starts after the mark has typed and keeps the product name as text", () => {
    render(<BrandName />);
    const el = screen.getByText("Harness");
    expect(el.style.getPropertyValue("--name-delay")).toBe(`${FIRST_TYPE_DELAY_MS + WORDMARK.length * TYPE_STEP_MS}ms`);
  });
});

describe("UltraTransition", () => {
  it("types ULTRA when engaging and STANDARD when leaving, never blocking input", () => {
    const { container, rerender } = render(<UltraTransition engaged origin={{ x: 1010, y: 46 }} />);
    const root = container.firstElementChild as HTMLElement;
    expect(root.getAttribute("aria-hidden")).toBe("true");
    expect(root.classList.contains("is-in")).toBe(true);
    expect(root.style.getPropertyValue("--ux")).toBe("1010px");
    expect(root.style.getPropertyValue("--uy")).toBe("46px");
    expect(root.querySelector(".ut-word b")?.textContent).toBe("ULTRA");
    expect(root.querySelectorAll(".ut-slats i")).toHaveLength(7);
    rerender(<UltraTransition engaged={false} origin={null} />);
    const out = container.firstElementChild as HTMLElement;
    expect(out.classList.contains("is-out")).toBe(true);
    expect(out.querySelector(".ut-word b")?.textContent).toBe("STANDARD");
    expect(out.querySelector(".ut-word small")?.textContent).toBe("VERIFICATION TIER OFFLINE");
  });

  it("falls back to the CSS default origin when none is known", () => {
    const { container } = render(<UltraTransition engaged origin={null} />);
    const root = container.firstElementChild as HTMLElement;
    expect(root.style.getPropertyValue("--ux")).toBe("");
    expect(root.classList.contains("no-origin")).toBe(true);
  });
});
