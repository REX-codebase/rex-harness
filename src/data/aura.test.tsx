// @vitest-environment jsdom
import { cleanup, fireEvent, render } from "@testing-library/react";
import { createRef, type RefObject } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { useAuraPointer, localPointer, originOf, GLOW_SELECTOR } from "./aura";

describe("localPointer", () => {
  const box = { left: 100, top: 50, width: 400, height: 200 };
  it("returns the pointer relative to the box", () => {
    expect(localPointer(box, 160, 90)).toEqual({ x: 60, y: 40 });
  });
  it("clamps to the box so the light never leaves its surface", () => {
    expect(localPointer(box, 20, 10)).toEqual({ x: 0, y: 0 });
    expect(localPointer(box, 900, 900)).toEqual({ x: 400, y: 200 });
  });
});

describe("originOf", () => {
  it("is the centre of the activated control, rounded", () => {
    expect(originOf({ left: 1000, top: 30, width: 71, height: 33 })).toEqual({ x: 1036, y: 47 });
  });
  it("does not depend on a pointer position, so keyboard activation works", () => {
    expect(originOf({ left: 0, top: 0, width: 0, height: 0 })).toEqual({ x: 0, y: 0 });
  });
});

function Shell({ shellRef }: { shellRef: RefObject<HTMLDivElement> }) {
  useAuraPointer(shellRef);
  return (
    <div ref={shellRef} data-testid="shell">
      <div className="task-field" data-testid="field"><textarea /></div>
      <p data-testid="plain">plain</p>
    </div>
  );
}

describe("useAuraPointer", () => {
  afterEach(() => { cleanup(); vi.restoreAllMocks(); });

  it("targets the working surfaces", () => {
    expect(GLOW_SELECTOR).toContain(".task-field");
    expect(GLOW_SELECTOR).toContain(".followup-field");
  });

  it("publishes the cursor and the surface-local position as CSS variables", async () => {
    vi.spyOn(window, "requestAnimationFrame").mockImplementation((cb) => { cb(0); return 1; });
    const ref = createRef<HTMLDivElement>();
    const { getByTestId } = render(<Shell shellRef={ref} />);
    const field = getByTestId("field");
    vi.spyOn(field, "getBoundingClientRect").mockReturnValue({ left: 100, top: 50, width: 400, height: 200, right: 500, bottom: 250, x: 100, y: 50, toJSON: () => ({}) });
    fireEvent.pointerMove(field, { clientX: 160, clientY: 90, pointerType: "mouse" });
    const shell = getByTestId("shell");
    expect(shell.style.getPropertyValue("--mx")).toBe("160px");
    expect(shell.style.getPropertyValue("--my")).toBe("90px");
    expect(shell.style.getPropertyValue("--mo")).toBe("1");
    expect(field.style.getPropertyValue("--px")).toBe("60px");
    expect(field.style.getPropertyValue("--py")).toBe("40px");
  });

  it("leaves non-surface elements untouched and fades the light when the pointer leaves", () => {
    vi.spyOn(window, "requestAnimationFrame").mockImplementation((cb) => { cb(0); return 1; });
    const ref = createRef<HTMLDivElement>();
    const { getByTestId } = render(<Shell shellRef={ref} />);
    const plain = getByTestId("plain");
    fireEvent.pointerMove(plain, { clientX: 10, clientY: 10, pointerType: "mouse" });
    expect(plain.style.getPropertyValue("--px")).toBe("");
    fireEvent.pointerLeave(getByTestId("shell"));
    expect(getByTestId("shell").style.getPropertyValue("--mo")).toBe("0");
  });

  it("ignores touch, which has no hover", () => {
    vi.spyOn(window, "requestAnimationFrame").mockImplementation((cb) => { cb(0); return 1; });
    const ref = createRef<HTMLDivElement>();
    const { getByTestId } = render(<Shell shellRef={ref} />);
    fireEvent.pointerMove(getByTestId("field"), { clientX: 5, clientY: 5, pointerType: "touch" });
    expect(getByTestId("shell").style.getPropertyValue("--mx")).toBe("");
  });
});
