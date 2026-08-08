import { describe, it, expect } from "vitest";
import { fmtMSS } from "./time";

describe("fmtMSS", () => {
  it("formats seconds as M:SS with zero-padded seconds", () => {
    expect(fmtMSS(0)).toBe("0:00");
    expect(fmtMSS(5)).toBe("0:05");
    expect(fmtMSS(65)).toBe("1:05");
    expect(fmtMSS(600)).toBe("10:00");
  });
  it("floors fractional seconds", () => {
    expect(fmtMSS(65.9)).toBe("1:05");
  });
  it("treats NaN / negative / 0-ish input as 0:00", () => {
    expect(fmtMSS(NaN)).toBe("0:00");
    expect(fmtMSS(-3)).toBe("0:00");
  });
});
