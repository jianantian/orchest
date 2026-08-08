import { describe, it, expect } from "vitest";
import { parseLRC, linesForRange } from "./lrc";

const lrc = parseLRC("[00:10.00]一\n[00:20.00]二\n[00:30.00]三\n[00:40.00]四");

describe("linesForRange", () => {
  it("选段覆盖的行 = 与 [start,end) 相交的行", () => {
    expect(linesForRange(lrc, 21, 35)).toEqual({ from: 2, to: 3 }); // 二[20,30) 三[30,40)
  });
  it("选段在两行之间也取相交行", () => {
    expect(linesForRange(lrc, 15, 25)).toEqual({ from: 1, to: 2 });
  });
  it("超出末尾 clamp 到最后一行", () => {
    expect(linesForRange(lrc, 35, 999)).toEqual({ from: 3, to: 4 });
  });
  it("空数组 / 选段全在第一行之前 → null 或首行", () => {
    expect(linesForRange([], 0, 10)).toBeNull();
    expect(linesForRange(lrc, 0, 5)).toBeNull(); // 第一行 10s 才开始
  });
});
