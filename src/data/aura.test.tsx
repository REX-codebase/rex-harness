import { describe, expect, it } from "vitest";
import { originOf } from "./aura";

describe("originOf", () => {
  it("is the centre of the activated control, rounded", () => {
    expect(originOf({ left: 1000, top: 30, width: 71, height: 33 })).toEqual({ x: 1036, y: 47 });
  });
  it("does not depend on a pointer position, so keyboard activation works", () => {
    expect(originOf({ left: 0, top: 0, width: 0, height: 0 })).toEqual({ x: 0, y: 0 });
  });
});
