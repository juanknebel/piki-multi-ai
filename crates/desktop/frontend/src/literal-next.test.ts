import { beforeEach, describe, expect, it } from "vitest";
import {
  armLiteralNext,
  consumeLiteralNext,
  disarmLiteralNext,
  isLiteralNextArmed,
  isLiteralPass,
  literalNextTab,
  literalNextVerdict,
  onLiteralNextChange,
  resetLiteralNext,
} from "./literal-next";

beforeEach(() => resetLiteralNext());

describe("literal-next", () => {
  it("arms for a tab and disarms", () => {
    expect(isLiteralNextArmed()).toBe(false);
    armLiteralNext("t1");
    expect(isLiteralNextArmed()).toBe(true);
    expect(literalNextTab()).toBe("t1");
    disarmLiteralNext();
    expect(isLiteralNextArmed()).toBe(false);
    expect(literalNextTab()).toBeNull();
  });

  it("modifier-only presses keep it armed, Escape cancels, anything else passes", () => {
    expect(literalNextVerdict({ key: "Control" })).toBe("modifier");
    expect(literalNextVerdict({ key: "Shift" })).toBe("modifier");
    expect(literalNextVerdict({ key: "Escape" })).toBe("cancel");
    expect(literalNextVerdict({ key: "b" })).toBe("pass");
    expect(literalNextVerdict({ key: "F" })).toBe("pass");
  });

  it("consume disarms and marks exactly that event as the pass-through", () => {
    armLiteralNext("t1");
    const ev = { key: "b" };
    expect(consumeLiteralNext(ev)).toBe("t1");
    expect(isLiteralNextArmed()).toBe(false);
    expect(isLiteralPass(ev)).toBe(true);
    expect(isLiteralPass({ key: "b" })).toBe(false); // a different event object
  });

  it("notifies listeners on arm and disarm only when the state changes", () => {
    const seen: (string | null)[] = [];
    const off = onLiteralNextChange((t) => seen.push(t));
    armLiteralNext("t1");
    armLiteralNext("t1"); // no-op
    disarmLiteralNext();
    disarmLiteralNext(); // no-op
    off();
    armLiteralNext("t2");
    expect(seen).toEqual(["t1", null]);
  });
});
