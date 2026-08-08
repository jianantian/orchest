import { describe, it, expect } from "vitest";
import { splitLines, spliceLines, lineRangeForSelection } from "./lyrics";

const L = "一\n二\n三\n四\n五";

describe("splitLines", () => {
  it("按 \\n 切分，保留空行", () => {
    expect(splitLines("a\n\nb\n")).toEqual(["a", "", "b", ""]);
  });
});

describe("spliceLines", () => {
  it("替换闭区间行", () => {
    expect(spliceLines(L, 2, 3, ["x", "y"])).toBe("一\nx\ny\n四\n五");
  });
  it("替换行数可与原区间不等", () => {
    expect(spliceLines(L, 2, 3, ["x"])).toBe("一\nx\n四\n五");
  });
  it("越界区间 clamp 到首尾", () => {
    expect(spliceLines(L, 0, 99, ["x"])).toBe("x");
  });
});

describe("lineRangeForSelection", () => {
  it("光标落在行内选中整行", () => {
    expect(lineRangeForSelection(L, 2, 4)).toEqual({ from: 2, to: 3 }); // selStart 在"二"，selEnd 在"三"
  });
  it("选区起止在同一行", () => {
    expect(lineRangeForSelection(L, 0, 1)).toEqual({ from: 1, to: 1 });
  });
  it("selEnd 恰为换行符时不多吃下一行", () => {
    expect(lineRangeForSelection(L, 2, 3)).toEqual({ from: 2, to: 2 }); // 选中"二\n"→ 仅第 2 行
  });
});
