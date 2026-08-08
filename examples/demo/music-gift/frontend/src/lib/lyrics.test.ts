import { describe, it, expect } from "vitest";
import { splitLines, spliceLines, lineRangeForSelection, manuscriptRangeForLrcRange } from "./lyrics";

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

const MANUSCRIPT = "[Verse]\n巷口的灯亮到第几盏\n\n你才慢吞吞地走回家\n[Chorus]\n我把祝福折成一只纸船";

describe("manuscriptRangeForLrcRange", () => {
  it("LRC 行号按可唱行序号映射回手稿行号（跳过空行与 [段落] 标记行）", () => {
    // 可唱行：2(巷口)、4(你才)、6(我把)；LRC 1–2 → 手稿 2–4
    expect(manuscriptRangeForLrcRange(MANUSCRIPT, { from: 1, to: 2 })).toEqual({ from: 2, to: 4 });
  });
  it("LRC 最后一行映射到最后一可唱行", () => {
    expect(manuscriptRangeForLrcRange(MANUSCRIPT, { from: 3, to: 3 })).toEqual({ from: 6, to: 6 });
  });
  it("LRC 行数超出可唱行数时 clamp", () => {
    expect(manuscriptRangeForLrcRange(MANUSCRIPT, { from: 1, to: 99 })).toEqual({ from: 2, to: 6 });
  });
  it("无可唱行（纯标记/空行）→ null", () => {
    expect(manuscriptRangeForLrcRange("[Verse]\n\n[Chorus]", { from: 1, to: 1 })).toBeNull();
  });
});
