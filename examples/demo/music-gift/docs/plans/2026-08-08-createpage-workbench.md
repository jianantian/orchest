# 创作页工作台（CreatePage Workbench）实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 按 `docs/design-system.md` §10 把 CreatePage 重建为宽双栏工作台：左手稿右产物、选区改稿（文本划选 + 音频选段）、take 试听区、移动端 dock/sheet、引导模式右栏搬家。

**Architecture:** 布局骨架先行（路由级宽度 + 双栏壳），功能逐层挂载（试听区 → 选区改稿前端 → scoped 后端协议 → 移动端 → 引导搬家）。视觉源材直接移植原型文件 `frontend/src/styles-prototype.css` 与 `frontend/src/pages/PrototypeStudioPage.tsx`（`pw-` 前缀类移植到 styles.css 时更名 `wb-`）。逻辑单元（歌词行工具、LRC 映射、scoped 解析）用新引入的 vitest + 现有 cargo test 做 TDD；UI 重组用 dev server + agent-browser 截图验证（项目既定模式）。

**Tech Stack:** React 19 + Vite 6 + TS（前端，新增 vitest）；Rust/axum（后端聊天链）；motion@13（已装，本期仅移动端 sheet 手势使用）。

## Global Constraints

- 样式只引用语义 token（`--surface/--accent/--r-lg` 等），**禁止字面 hex/rgba**；圆角只用五档 `--r-sm/md/lg/xl/pill`（design-system §3）。
- 动效只用 §4.2 四档弹簧预设（`spring-default/snap/sheet/pop`）或 CSS transform/opacity；reduced-motion 一律降级交叉淡入。
- 新文案进 `i18n.tsx` 五语言（zh/en/fr/es/ru，各语言 keys 必须对齐，模块加载有断言）；新文案零 em-dash。
- 可交互元素用原生 `<button>`（design-system §5 / D19 P0）。
- Rust 侧：`unwrap()/expect()` 禁止出现在非 test 代码；现有测试模式（`agent.rs` 内 `#[cfg(test)]` + mock model）沿用。
- 提交规范：`feat:` 前缀，一个 Task 一个 commit（WORKFLOW.md）；**执行 git commit 前需用户确认**（会话规则）。
- 视觉验收基准：原型 `/prototype/studio` 三视图（实现完成后原型删除，见 Task F）。

---

## Phase A · 布局骨架（桌面双栏 + 路由级宽度）

### Task A1: 路由级框架宽度机制

**Files:**
- Modify: `frontend/src/App.tsx`（Routes 区域）
- Modify: `frontend/src/styles.css:149-167`（`#root` 宽度规则块）

**Interfaces:**
- Produces: `#root.frame-home` CSS 钩子（后续所有 workbench 布局依赖它在 ≥1100px 时为 1040px 宽）。

- [ ] **Step 1: App.tsx 给 #root 打路由级 class**

`App.tsx` 顶部加 `import { useLocation } from "react-router-dom";`，在 `AppContent` 内：

```tsx
const { pathname } = useLocation();
useEffect(() => {
  const root = document.getElementById("root");
  root?.classList.toggle("frame-home", pathname === "/");
  return () => root?.classList.remove("frame-home");
}, [pathname]);
```

（`useEffect` 需加入现有 import。不新建组件，直接操作 #root 是因为 frame 在 App 之外。）

- [ ] **Step 2: styles.css 加宽度规则**

在 `styles.css:165-167` 的 `@media (min-width: 900px)` 块**之后**新增（注意不得插入 dark token 覆盖块之后，见该文件注释约定——新增规则必须位于 dark 覆盖块 `:1833+` 之前）：

```css
/* Workbench: create page widens to a two-column frame on large screens.
   Route-level only — the frame never changes width within a page. */
@media (min-width: 1100px) {
  #root.frame-home { max-width: 1040px; }
}
#root { transition: max-width var(--panel-dur, 0.3s) var(--panel-ease, cubic-bezier(0.22, 1, 0.36, 1)); }
```

（`--panel-dur/--panel-ease` 为 Phase 1 已有动效 token，先在 styles.css token 区确认确切变量名，不同则改用现有名。）

- [ ] **Step 3: 验证**

`npm run dev -- --port 5203` 后台起服，`agent-browser --session wb open http://localhost:5203/`（1280px 视口）截图确认创作页 1040 宽、`/playlist` 仍 720；切路由无跳变。
Expected: 创作页宽 1040，其余页 720。

- [ ] **Step 4: Commit**（经用户确认后）`feat: add route-level frame width for create workbench`

### Task A2: 工作台顶栏（tabs + 草稿状态 + studio-only undo）

**Files:**
- Modify: `frontend/src/pages/CreatePage.tsx`（全文件重写 JSX 壳，保留 `?edit=` 逻辑）
- Modify: `frontend/src/styles.css`（新增 `wb-topbar` 族）

**Interfaces:**
- Consumes: Task A1 的 `frame-home`。
- Produces: `WorkbenchTopbar({ active, showTabs, undoSlot })`（CreatePage 内联组件）；`active: "guided"|"studio"`；edit 模式 `showTabs=false`。undo 按钮的实际 handler 由 Task A3 经 render prop 注入。

- [ ] **Step 1: 移植顶栏 CSS**

从 `styles-prototype.css` 移植 `.pw-topbar/.pw-tabs/.pw-tab(.on)/.pw-topbar-actions/.pw-draft-state` 到 `styles.css`，更名 `.wb-topbar/.wb-tabs/.wb-tab(.on)/.wb-topbar-actions/.wb-draft-state`，删除原型特有的固定 padding 外（沿用原型值 10px 20px）。同时移植 `.pw-icon-btn` → `.wb-icon-btn`。

- [ ] **Step 2: 重写 CreatePage 壳**

```tsx
// CreatePage.tsx — workbench shell
export default function CreatePage() {
  const [searchParams] = useSearchParams();
  const editId = searchParams.get("edit");
  const [tab, setTab] = useState<"guided" | "studio">(editId ? "studio" : "guided");
  // edit 模式语义不变：强制 studio、不显示 tab 切换（沿用现 :27-33 行为）
  return (
    <div className="wb-bench">
      <div className="wb-topbar">
        {!editId ? (
          <div className="wb-tabs">
            <button className={`wb-tab${tab === "guided" ? " on" : ""}`} onClick={() => setTab("guided")}>{t("tab_guided")}</button>
            <button className={`wb-tab${tab === "studio" ? " on" : ""}`} onClick={() => setTab("studio")}>{t("tab_free")}</button>
          </div>
        ) : <span />}
        <div className="wb-topbar-actions">{/* 草稿状态文案由 Task A3/A5 接线 */}</div>
      </div>
      {tab === "guided" && !editId
        ? <GuidedFlow onNavigate={...} onSwitchToFree={() => setTab("studio")} />
        : <Studio lang={lang} photos={[]} onNavigate={...} editGiftId={editId ?? undefined} />}
    </div>
  );
}
```

`.wb-bench` CSS 移植自原型 `.pw-bench`（去掉 width 1040——宽度由 frame-home 管）。tab 切换内容交叉替换：两栏内容容器加 `transition: opacity 150ms ease-out`，切 tab 时先 opacity 0 再挂载新内容（reduced-motion 下无过渡直接切换）。

- [ ] **Step 3: i18n 检查** — `tab_guided/tab_free` 已存在，复用；无新文案。
- [ ] **Step 4: 截图验证**（两 tab 切换、edit 模式无 tabs）。
- [ ] **Step 5: Commit** `feat: workbench topbar with mode tabs`

### Task A3: Studio 桌面双栏重排

**Files:**
- Modify: `frontend/src/components/Studio.tsx`（JSX 重排为双栏；逻辑全保留）
- Create: `frontend/src/components/studio/StyleCard.tsx`
- Modify: `frontend/src/styles.css`（移植卡片语言）

**Interfaces:**
- Consumes: Task A2 壳。
- Produces: `.wb-card/.wb-doc/.wb-doc-divider/.wb-cols/.wb-col(-driver|-artifact)/.wb-chips/.wb-chip(.plain)/.wb-action-row`；Studio 右栏容器 `div.wb-col-artifact`（Task B/C 往里挂试听卡/版本卡/AI 卡）。

- [ ] **Step 1: 移植卡片 CSS**

从 `styles-prototype.css` 移植并更名：`.pw-card→.wb-card`、`.pw-cols→.wb-cols`、`.pw-col→.wb-col`、`.pw-col-driver→.wb-col-driver`、`.pw-col-artifact→.wb-col-artifact`、`.pw-doc/.pw-doc-divider/.pw-title-input`（保留现有 `.title-input` 类名映射到 `.wb-doc` 内样式，避免双套）、`.pw-chips/.pw-chip(.plain)→.wb-chips/.wb-chip(.plain)`、`.pw-action-row→.wb-action-row`、`.pw-btn*`（对照现有 `.btn-primary/.btn-secondary` 合并，不新增按钮体系——§5 三档按钮已存在，只取原型的尺寸值）。**不移植**：`.pw-player/.pw-take*/.pw-chat*/.pw-sheet/.pw-dock`（后续 Task 各自移植）。

- [ ] **Step 2: Studio.tsx JSX 重排**

现有 `div.studio > div.free-panel.editorial`（:523-653）内容映射为：

```tsx
<div className="wb-cols">
  <div className="wb-col wb-col-driver">
    <div className="wb-card wb-doc">
      <input className="title-input" ... />   // 现 :567，原样
      <hr className="wb-doc-divider" />
      <textarea className="lyrics-manuscript" ... />  // 现 :573-575，原样（保留 ✨帮写/空态 CTA，:576-578）
    </div>
    <StyleCard ... />                          // 现风格 section :584-597 + 更多选项 :603-624 合并
    <div className="wb-action-row">            // 现 :628-652 操作区，原样逻辑
      {/* edit 模式双按钮 / 新建模式生成按钮 */}
    </div>
  </div>
  <div className="wb-col wb-col-artifact">
    {/* Task B 挂试听卡/版本卡；AI chat-panel :658-713 暂存于此，Task A3 内先原样搬入 */}
  </div>
</div>
```

DraftSection 折叠语义在创作室废弃（卡常驻展开）；`DraftSection` 组件保留给后续 review/其他调用方（确认无其他引用后可删，grep 决定）。edit-head（:527-565）**本 Task 不动**，留在 driver 栏顶部原位，Task B1 移入产物区。

- [ ] **Step 3: StyleCard.tsx**

纯搬运：风格 chips + 建议 pills + ↻ + 演唱方式/人声两行（现 :584-624），props 从 Studio 注入（`selectedStyles/styleInput/suggestions/instrumental/vocalGender` 及各 setter——按现有函数签名传递，不改逻辑）。

- [ ] **Step 4: 截图对比验证**——双栏布局成立、两 tab 切换正常、edit 模式（`/ ?edit=<id>`，需本地有 gift）回显正常。
- [ ] **Step 5: Commit** `feat: studio two-column workbench layout`

---

## Phase B · 试听区（player + takes）

### Task B1: 试听卡 + 版本卡（edit 模式）

**Files:**
- Create: `frontend/src/components/studio/PlayerCard.tsx`
- Create: `frontend/src/components/studio/TakesCard.tsx`
- Modify: `frontend/src/components/Studio.tsx:527-565`（edit-head 移除，状态下沉到两卡）
- Modify: `frontend/src/styles.css`（移植 `.pw-player*/.pw-take*/.pw-disc` → `.wb-player*/.wb-take*/.wb-disc`）

**Interfaces:**
- Produces:
  - `PlayerCard({ audioUrl, coverUrl, title, versionLabel, onTimeUpdate }: { audioUrl: string; coverUrl: string|null; title?: string; versionLabel: string; onTimeUpdate: (t: number) => void })` — Task C4 往里加选段。
  - `TakesCard({ versions, currentIdx, onSelect, onBranch }: { versions: GiftVersion[]; currentIdx: number; onSelect: (i: number) => void; onBranch: (i: number) => void })` — `onBranch` = 现 `loadVersionToDraft`（Studio.tsx:310）。

- [ ] **Step 1: 移植 CSS**（`.pw-player/.pw-disc/.pw-player-meta/.pw-player-title/.pw-player-sub/.pw-takes-card/.pw-take(.on)/.pw-take-badge/.pw-take-meta/.pw-take-name/.pw-take-sub/.pw-take-actions` → `wb-`）。
- [ ] **Step 2: PlayerCard.tsx** — 碟片 + 标题/版本行 + 复用现有 `<AudioPlayer src title onTimeUpdate>`（Studio.tsx:534 原样移入）；regen==="generating" 时显示现有 polish-status spinner 语义（:529-531）。
- [ ] **Step 3: TakesCard.tsx** — versions.map 渲染 take 行（badge `V{v.version}`、`v === currentIdx` 高亮 `.wb-take.on`、▶ 试听 = onSelect、⑂ 分叉 = onBranch）；分叉按钮文案 `t("load_to_draft")` 复用现有 i18n key；新增「分叉」语义不入文案（icon-only + title 属性）。图标：▶ 用文本占位禁止——按 §10.9 补 `PlayIcon/BranchIcon` 两个 inline SVG 进 `Icons.tsx`（仿现有 6 个图标的 2 行函数式写法）。
- [ ] **Step 4: Studio.tsx 接线** — edit-head（:527-565）删除，产物区渲染 `<PlayerCard .../>` + `<TakesCard .../>`；`shownAudio/shownVersion/versionIdx` 等派生（:493-501）原样复用；LRC 歌词展示区（:554-563）保留在试听卡内（edit-lyrics 样式原样）。
- [ ] **Step 5: 截图验证**（edit 模式右栏两卡；分叉后手稿载入旧版字段）。
- [ ] **Step 6: Commit** `feat: player and takes cards in studio artifact column`

### Task B2: 新建模式生成后产物区

**Files:**
- Modify: `frontend/src/components/Studio.tsx`（新建分支 :643-652）
- Modify: `frontend/src/hooks/useMusicGen.ts`（无需改签名，仅消费）

**Interfaces:**
- Consumes: `gen.state/giftId/error`（useMusicGen.ts:157）；Task B1 的 PlayerCard。

- [ ] **Step 1:** 新建模式 `gen.giftId` 非空时，产物区渲染：生成中 = 现有 `<MusicCard>`（:650 原样搬入右栏）；`gen.state === "ready"` 时 `getGift(gen.giftId)` 拉取后渲染 `<PlayerCard audioUrl={gift.audio_url} .../>` + 「打开礼物页」次级按钮（保留现 `onNavigate` 出口，spec §10.9：不再自动跳走）。
- [ ] **Step 2:** MusicCard 的 `onOpen` 改为次级按钮样式，不再占主 CTA 位。
- [ ] **Step 3: 截图验证**（完整新建链路：填词 → 生成 → 右栏出进度 → ready 出播放器）。
- [ ] **Step 4: Commit** `feat: artifact column shows generation result in place`

---

## Phase C · 选区改稿（前端 + scoped 后端）

### Task C1: vitest 基建 + 歌词行/LRC 映射纯逻辑

**Files:**
- Modify: `frontend/package.json`（devDeps + script）
- Create: `frontend/src/lib/lyrics.ts`
- Create: `frontend/src/lib/lyrics.test.ts`
- Modify: `frontend/src/lib/lrc.ts`（追加 `linesForRange`）
- Modify: `frontend/src/lib/lrc.test.ts`（新建；现有无此文件）

**Interfaces:**
- Produces（后续 Task 的唯一真源，签名不得改）:

```ts
// lib/lyrics.ts — 行号全部 1-based、闭区间
export function splitLines(lyrics: string): string[];
export function spliceLines(lyrics: string, from: number, to: number, replacement: string[]): string;
export function lineRangeForSelection(text: string, selStart: number, selEnd: number): { from: number; to: number };
// lib/lrc.ts
export function linesForRange(lines: LRCLine[], startSec: number, endSec: number): { from: number; to: number } | null;
```

- [ ] **Step 1: 装 vitest**

```bash
cd frontend && npm install -D vitest
```

`package.json` scripts 加 `"test": "vitest run"`。跑 `npm test` 确认 0 tests found 不报错（passWithNoTests 默认行为，若报错则加 `--passWithNoTests`）。

- [ ] **Step 2: 写失败测试 `lib/lyrics.test.ts`**

```ts
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
```

- [ ] **Step 3: 跑 `npm test` 确认 FAIL**（模块不存在）。
- [ ] **Step 4: 实现 `lib/lyrics.ts`**

```ts
/** 歌词行工具。行号约定：1-based、闭区间（与 UI 文案"第 N–M 行"一致）。 */

export function splitLines(lyrics: string): string[] {
  return lyrics.split("\n");
}

/** 把 lyrics 的第 from–to 行（1-based 闭区间，越界 clamp）替换为 replacement 行数组。 */
export function spliceLines(lyrics: string, from: number, to: number, replacement: string[]): string {
  const lines = splitLines(lyrics);
  const f = Math.min(Math.max(from, 1), lines.length);
  const t = Math.min(Math.max(to, f), lines.length);
  lines.splice(f - 1, t - f + 1, ...replacement);
  return lines.join("\n");
}

/** 把 textarea 的 selectionStart/End 映射为覆盖的整行范围（1-based 闭区间）。 */
export function lineRangeForSelection(text: string, selStart: number, selEnd: number): { from: number; to: number } {
  const from = text.slice(0, selStart).split("\n").length;
  // selEnd 恰落在换行符上时，选区不含下一行
  const effectiveEnd = selEnd > selStart && text[selEnd - 1] === "\n" ? selEnd - 1 : selEnd;
  const to = text.slice(0, Math.max(effectiveEnd, selStart)).split("\n").length;
  return { from, to: Math.max(to, from) };
}
```

- [ ] **Step 5: `lib/lrc.test.ts` 追加**

```ts
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
```

语义：第 i 行覆盖区间 `[lines[i].time, lines[i+1].time)`（末行到 +∞）；选段 `[startSec,endSec)` 与其相交的行入范围；`endSec <= lines[0].time` 或空数组 → null。

- [ ] **Step 6: 跑 `npm test` 确认 FAIL，再在 `lib/lrc.ts` 末尾追加实现**

```ts
/** 把音频时间选段 [startSec,endSec) 映射到覆盖的 LRC 行范围（1-based 闭区间）。
 *  第 i 行覆盖 [lines[i].time, lines[i+1].time)。选段全在首行之前或 lines 为空 → null。 */
export function linesForRange(lines: LRCLine[], startSec: number, endSec: number): { from: number; to: number } | null {
  if (lines.length === 0 || endSec <= lines[0].time) return null;
  let from = -1, to = -1;
  for (let i = 0; i < lines.length; i++) {
    const lineStart = lines[i].time;
    const lineEnd = i + 1 < lines.length ? lines[i + 1].time : Infinity;
    if (lineStart < endSec && startSec < lineEnd) {
      if (from === -1) from = i + 1;
      to = i + 1;
    }
  }
  return from === -1 ? null : { from, to };
}
```

- [ ] **Step 7: `npm test` 全 PASS；Commit** `feat: lyric line utils and LRC range mapping with vitest`

### Task C2: 手稿划选 → 浮动工具条

**Files:**
- Create: `frontend/src/components/studio/SelectionToolbar.tsx`
- Modify: `frontend/src/components/Studio.tsx`（手稿卡接入）
- Modify: `frontend/src/styles.css`（移植 `.pw-seltoolbar` → `.wb-seltoolbar`）
- Modify: `frontend/src/i18n.tsx`（新文案五语言）

**Interfaces:**
- Consumes: `lineRangeForSelection`（Task C1）。
- Produces:
  - `SelectionToolbar({ onAction }: { onAction: (cmd: ScopedCommand) => void })`，`type ScopedCommand = "rewrite" | "rhyme" | "colloquial" | "shorten" | "custom"`。
  - Studio 内部状态 `selRange: { from: number; to: number } | null`（Task C3/C4 共用）。

- [ ] **Step 1: i18n 五语言新 keys**（scoped_rewrite/scoped_rhyme/scoped_colloquial/scoped_shorten/scoped_custom → zh 改写/更押韵/更口语/缩短/自定义指令…，其余四语言按现有翻译风格补齐）。
- [ ] **Step 2: SelectionToolbar.tsx** — 深色 pill（CSS 移植原型），五个 `<button>`（原生，§5）；`custom` 点击暂发 `"custom"`（Task C5 接自定义输入；本期 custom 落到聊天输入框预填，见 Step 4）。
- [ ] **Step 3: Studio.tsx 手稿 textarea 接 selection**

`lyrics-manuscript` textarea 加 `onSelect`：`const ta = e.currentTarget; const { from, to } = lineRangeForSelection(ta.value, ta.selectionStart, ta.selectionEnd); setSelRange(ta.selectionEnd > ta.selectionStart ? { from, to } : null);`（blur/点击别处时清空：在手稿卡 onBlur 延迟 150ms 判定，避免点工具条时选区先消失）。工具条绝对定位在手稿卡内（CSS 原型位）。

- [ ] **Step 4:** `onAction` 暂不请求 AI——本 Task 只把选区行高亮（`.wb-line.sel` 类，移植 `.pw-line(.sel)`）+ 工具条显隐 + custom 落聊天输入框预填。AI 接线在 Task C5。
- [ ] **Step 5: 截图验证**（划选 → 行高亮 + 工具条出现；点击空白消失）。
- [ ] **Step 6: Commit** `feat: lyric selection toolbar in manuscript`

### Task C3: 行内 diff 提案组件 + undo 集成

**Files:**
- Create: `frontend/src/components/studio/LyricsProposal.tsx`
- Modify: `frontend/src/components/Studio.tsx`（proposal state + accept/reject）
- Modify: `frontend/src/styles.css`（移植 `.pw-diff*` → `.wb-diff*`）

**Interfaces:**
- Produces:
  - `interface Proposal { from: number; to: number; original: string[]; replacement: string[] }`
  - `LyricsProposal({ proposal, onAccept, onReject }: { proposal: Proposal; onAccept: () => void; onReject: () => void })`
  - Studio state `proposal: Proposal | null`（Task C5 由 Done.lines 生成）。

- [ ] **Step 1: 移植 CSS**（`.pw-diff/.pw-diff-old/.pw-diff-new/.pw-diff-actions/.pw-diff-note` → `wb-`；danger 淡底用 Phase 1 已有 `--danger-tint-8` 系 token 替换原型的 color-mix 字面量）。
- [ ] **Step 2: LyricsProposal.tsx** — old 行（划线）/ new 行（高亮）/ 操作条（`AI 提案 · 第 N–M 行` + ✓接受/✕拒绝 两原生按钮）；出现动画 grid-rows 展开（spring-default 语义用 CSS 过渡近似，reduced-motion 降级淡入）。
- [ ] **Step 3: Studio.tsx 集成**

手稿渲染从单 textarea 变为「提案态 = 只读行视图 + diff 块」：有 proposal 时手稿卡内渲染行列表（proposal 区间位置渲染 LyricsProposal），无 proposal 时保持 textarea 编辑。accept：`pushUndo(draftRef.current); setLyrics(spliceLines(lyrics, p.from, p.to, p.replacement)); setProposal(null);`；reject：`setProposal(null)`。undo 按钮（现有 :664）语义不变。

- [ ] **Step 4: 用假数据截图验证**（临时 useState 注入一个 Proposal 常量，截图后删除）。
- [ ] **Step 5: Commit** `feat: inline diff proposal component with accept/reject`

### Task C4: 播放器选段 → LRC 映射

**Files:**
- Create: `frontend/src/components/studio/RangeSelect.tsx`
- Modify: `frontend/src/components/studio/PlayerCard.tsx`（挂入选段层）
- Modify: `frontend/src/components/Studio.tsx`（映射 + 「交给 AI 修改」）
- Modify: `frontend/src/styles.css`（移植 `.pw-range*` → `.wb-range*`）
- Modify: `frontend/src/i18n.tsx`（新文案）

**Interfaces:**
- Consumes: `linesForRange`（Task C1）；Studio 的 `selRange`（Task C2）。
- Produces: `RangeSelect({ duration, value, onChange }: { duration: number; value: { start: number; end: number } | null; onChange: (v: { start: number; end: number } | null) => void })`。

- [ ] **Step 1: RangeSelect.tsx** — progress bar 覆盖层：双 handle（pointerdown + setPointerCapture 1:1 跟踪，§4.2）、选段高亮带、边界橡皮筋（拖过 0/duration 时按 `(x*d*0.55)/(d+0.55*|x|)` 衰减视觉位移，松手 clamp）；`onChange` 提交秒值。duration 从 AudioPlayer 的 audio 元素 `loadedmetadata` 获得（PlayerCard 内 ref 透传，不用 `document.querySelector`——§7 P2 明确禁止）。
- [ ] **Step 2: PlayerCard 集成** — progress 区渲染 RangeSelect；`onTimeUpdate` 已存在。
- [ ] **Step 3: Studio.tsx** — 选段 onChange → `linesForRange(shownLrcLines, v.start, v.end)` → 显示「已选 M:SS–M:SS → 对应歌词第 N–M 行」（i18n 插值；`linesForRange` 返回 null 或纯音乐无 LRC 时隐藏映射半句，§10.10）→「交给 AI 修改」按钮 = `setSelRange(mapped)` + 滚动手稿卡入视 + 高亮对应行（复用 Task C2 高亮机制）。
- [ ] **Step 4: i18n keys**（range_selected/range_mapped_to_lines/range_send_to_ai，五语言）。
- [ ] **Step 5: 截图验证**（edit 模式拖选段 → 映射文案 → 点按钮手稿行高亮）。
- [ ] **Step 6: Commit** `feat: audio range selection mapped to lyric lines`

### Task C5: scoped 后端协议 + 前端接线

**Files:**
- Create: `prompts/studio_scoped.md`
- Modify: `src/prompts.rs:18` 附近（新增 static include）
- Modify: `src/agent/message.rs:97-124`（ChatRequest.selection + scoped system message）
- Modify: `src/routes.rs:124-131`（scoped 分流）
- Modify: `src/agent.rs`（`parse_studio_output` :70 + `SseEvent::Done` :215-229 + `build_done_event` :534）
- Modify: `frontend/src/types.ts:25-62`（ChatRequest.selection、Done.lines）
- Modify: `frontend/src/components/Studio.tsx`（sendScopedTurn）
- Test: `src/agent.rs` 内 `#[cfg(test)]` 追加；`src/agent/message.rs` 测试追加

**Interfaces:**
- Consumes: Task C2 `ScopedCommand`、Task C3 `Proposal`。
- Produces（协议，前后端一致）:
  - 请求：`ChatRequest.selection?: { from: number; to: number }`（1-based 闭区间，指向 `draft.lyrics` 行）+ `mode: "studio"`（复用 studio 分支，selection 存在即 scoped 语义）。
  - 响应：模型输出末尾 `<<<LINES:N-M>>>\n<替换文本>\n<<<END>>>`；`SseEvent::Done` 新增 `lines?: { from: number; to: number; text: string } | null`。

- [ ] **Step 1: 写 prompts/studio_scoped.md**

基于 `prompts/studio.md:27-46` 的 marker 惯例改写：注入"用户选中了第 N–M 行（附行内容）+ 全稿上下文"，要求：只重写选中行、保持与上下文的韵脚/节拍一致、替换文本用 `<<<LINES:N-M>>>` 包裹、聊天正文照常先出。语言跟随请求 lang。

- [ ] **Step 2: message.rs — ChatRequest 加字段 + scoped 系统消息**

```rust
// ChatRequest 加：
pub selection: Option<Selection>,
#[derive(Debug, Deserialize)]
pub struct Selection { pub from: usize, pub to: usize }
```

新增 `build_scoped_system_message(draft, selection, scoped_prompt)`：在 `build_studio_system_message` 基础上追加 "Selected lines N–M:" 段（行内容从 draft.lyrics 按 1-based 取出，越界 clamp）。

测试（仿 `message.rs:175-269` 现有 studio 测试）：含 selection 的请求序列化往返、系统消息含选中行原文。

- [ ] **Step 3: routes.rs 分流** — `studio` 分支内：`match req.selection { Some(sel) => build_scoped_system_message(...), None => build_studio_system_message(...) }`。

- [ ] **Step 4: agent.rs — 解析 + Done 字段**

`SseEvent::Done`（:215-229）加 `pub lines: Option<LinesChange>`；`pub struct LinesChange { pub from: usize, pub to: usize, pub text: String }`。`parse_studio_output`（:70-88）加 `extract_lines_block`：找 `<<<LINES:(\d+)-(\d+)>>>` 到 `<<<END>>>` 或文末，解析 N/M 与正文（trim 尾部空行）。

测试（仿 agent.rs:573+ 现有 parse 测试矩阵）：正常块 / 缺 END 到文末 / 无块 → None / 数字逆序（5-3）→ None。

- [ ] **Step 5: `cargo test`（music-gift crate）全 PASS**。
- [ ] **Step 6: 前端 types.ts** — `ChatRequest.selection?: { from: number; to: number }`；`Done` 变体加 `lines?: { from: number; to: number; text: string } | null`（api.ts 透传无需改）。
- [ ] **Step 7: Studio.tsx sendScopedTurn**

```ts
async function sendScopedTurn(cmd: ScopedCommand, range: { from: number; to: number }) {
  const instruction = t(`scoped_prompt_${cmd}`); // i18n 指令模板，五语言
  const msgs: StudioMessage[] = [...messages, { role: "user", content: instruction }];
  setMessages(msgs); setStreaming(true);
  let arrived = "";
  const gen = streamChat({ mode: "studio", draft: draftForApi(), selection: range, messages: msgs, meta: { lang }, photos: [] });
  for await (const e of gen) {
    if (e.type === "Delta") { arrived += e.text; setMessages([...msgs, { role: "assistant", content: arrived }]); }
    else if (e.type === "Done" && e.lines) {
      const original = splitLines(draftRef.current.lyrics).slice(e.lines.from - 1, e.lines.to);
      setProposal({ from: e.lines.from, to: e.lines.to, original, replacement: splitLines(e.lines.text) });
    }
  }
  setStreaming(false);
}
```

工具条 `onAction(cmd)` 接线：`cmd === "custom"` → 聊天输入框 focus + 预填（自定义指令走普通 scoped 发送，输入框 onSubmit 时若 selRange 非空则随消息带 selection）；其余直接 `sendScopedTurn(cmd, selRange)`。

- [ ] **Step 7b: 聊天整字段改稿同样落 diff（§10.5）**

`applyDone`（Studio.tsx:371）分流：`e.lyrics != null` 时**不再直接 setLyrics**，改为生成整稿提案 `setProposal({ from: 1, to: splitLines(draftRef.current.lyrics).length, original: splitLines(draftRef.current.lyrics), replacement: splitLines(e.lyrics) })`（接受/拒绝/undo 复用 Task C3 机制）；`style/title/vocal` 是单值字段无 diff 概念，维持直接应用 + 字段高亮（现有行为）。

- [ ] **Step 8: i18n keys**（scoped_prompt_* 指令模板五语言）。
- [ ] **Step 9: 端到端验证**（需 backend 跑着 + 模型配置；划选 → 改写 → diff 出现 → 接受 → undo）。
- [ ] **Step 10: Commit** `feat: scoped lyric editing protocol end to end`

---

## Phase D · 移动端（dock + sheet）

### Task D1: 吸底迷你播放器 + bottom sheet

**Files:**
- Create: `frontend/src/components/studio/MiniPlayerDock.tsx`
- Create: `frontend/src/components/studio/ListenSheet.tsx`
- Modify: `frontend/src/components/Studio.tsx`（移动断点渲染分支）
- Modify: `frontend/src/styles.css`（移植 `.pw-dock/.pw-dim/.pw-sheet*` → `.wb-dock/.wb-dim/.wb-sheet*`；`@media (max-width: 1099px)` 单栏规则）

**Interfaces:**
- Consumes: PlayerCard/TakesCard/ChatCard（产物区组件原样复用进 sheet）；motion@13。

- [ ] **Step 1: 移动单栏 CSS** — `@media (max-width: 1099px)` 下 `.wb-cols { grid-template-columns: 1fr }`，`.wb-col-artifact { display: none }`（内容移入 sheet，由 React 分支渲染，不用 CSS 隐藏——直接条件渲染避免双份 DOM）。
- [ ] **Step 2: MiniPlayerDock.tsx** — glass 材质（CSS 移植原型）；碟片 + 标题 + 细进度条（复用 AudioPlayer 的 time 状态，Studio 层共享 audio ref）+ ▶ + ⌃；有 audioUrl 才渲染。
- [ ] **Step 3: ListenSheet.tsx** — `motion/react` 的 `drag="y"` + `dragConstraints={{ top: 0 }}` + 松手速度决定展开/收起（速度符号判定，§4.2）；动画 preset `spring-sheet` 参数 `{ type: "spring", bounce: 0.2, duration: 0.35 }`；遮罩 `.wb-dim` 点击收起；reduced-motion 降级 fade。sheet 内容 = `<PlayerCard/><TakesCard/><AI 卡>`。
- [ ] **Step 4: 截图验证**（390px 视口：dock 常驻、⌃ 上拉 sheet、拖拽可中断反向）。
- [ ] **Step 5: Commit** `feat: mobile mini player dock and listen sheet`

### Task D2: 移动端细节

**Files:**
- Modify: `frontend/src/styles.css`（紧凑工具条、移动 CTA）
- Modify: `frontend/src/components/Studio.tsx`（undo 条件渲染）
- Modify: `frontend/src/i18n.tsx`（短版指令 keys）

- [ ] **Step 1:** 紧凑工具条（`.wb-seltoolbar.compact`，移植原型；文案用短版 i18n keys）。
- [ ] **Step 2:** undo 入口移动：移动断点下从顶栏移入 AI 协作卡头部（媒体查询 + 条件渲染）。
- [ ] **Step 3:** 全宽 CTA（`.wb-action-row` 移动下 flex column，主按钮 `width: 100%`）。
- [ ] **Step 4: Commit** `feat: mobile workbench details`

---

## Phase E · 引导模式右栏搬家

### Task E1: 需求卡（BriefCard）

**Files:**
- Create: `frontend/src/components/guided/BriefCard.tsx`
- Modify: `frontend/src/components/GuidedFlow.tsx`
- Modify: `frontend/src/i18n.tsx`

**Interfaces:**
- Consumes: useGuidedState 的 `FlowStep` 与各采集字段（useGuidedState.ts:13 状态机；relationship/name/gender/birthday/scenario/vocal 字段名以该文件实际为准）。

- [ ] **Step 1: BriefCard.tsx** — `dl` 四~六行（送给/场合/氛围/人声），未采集行显示占位 `--`；行出现 stagger 60ms。
- [ ] **Step 2: i18n keys**（brief_to/brief_occasion/brief_mood/brief_vocal 等，五语言）。
- [ ] **Step 3: Commit** `feat: guided brief card`

### Task E2: ReviewCard/MusicCard 移出聊天流

**Files:**
- Modify: `frontend/src/components/GuidedFlow.tsx:445-446`（两处内联渲染移除）
- Modify: `frontend/src/pages/CreatePage.tsx`（guided 模式双栏壳：左对话卡右产物区）

**Interfaces:**
- Consumes: Task A3 的 `.wb-cols` 壳与卡片 CSS；现有 `ReviewCard`/`MusicCard` props（ReviewCard.tsx:11-26、MusicCard.tsx:6-11）原样。

- [ ] **Step 1:** CreatePage guided 分支套双栏：左 `.wb-card` 包对话流（气泡 + PillsRow 钉输入条上方——ChatUI 的 PillsRow 现渲染位置随聊天流，移到输入条上方固定区），右 `.wb-col-artifact`：`<BriefCard/>` 常驻；`step === "review"` 渲染 `<ReviewCard>`（props 从 GuidedFlow 现有 :445 处原样上移，经 callback/state 提升——GuidedFlow 需把 `draft/review/degraded/onSubmit/onOpenInStudio` 暴露给 CreatePage：给 GuidedFlow 加 `artifactSlot?: (node: ReactNode) => void` 或直接把右栏收进 GuidedFlow 渲染（更简单：GuidedFlow 自己套双栏，CreatePage 不传栏）。**取后者**：GuidedFlow 内部套 `.wb-cols`，CreatePage 无感知）。
- [ ] **Step 2:** `step === "music"` 渲染 `<MusicCard>` 到右栏（:446 上移）；「在创作室中打开」交接（:329-332 `stageStudioDraft` + `onSwitchToFree`）不动。
- [ ] **Step 3: 截图验证**（引导全流程：pills 在输入条上方、review/music 出现在右栏、聊天流无卡片）。
- [ ] **Step 4: Commit** `feat: guided artifact column with brief review music cards`

---

## Phase F · 收尾

### Task F: 原型吸收清理 + 文档状态

- [ ] **Step 1:** 删除 `frontend/src/pages/PrototypeStudioPage.tsx`、`frontend/src/components/PrototypeSwitcher.tsx`、`frontend/src/styles-prototype.css`，移除 `App.tsx` 的 `/prototype/studio` 路由与 import。
- [ ] **Step 2:** 全文 grep 确认 `pw-` 零残留、`prototype` 零残留（styles.css 内 wb- 类均为移植后正式代码）。
- [ ] **Step 3:** `docs/todo.md` #2 勾掉并标注实现日期；`design-system.md` §8 Phase 3 标 ✅。
- [ ] **Step 4: Commit** `feat: absorb workbench prototype, mark todo done`

---

## 全局验证（Phase F 后）

- `cd frontend && npm test && npx tsc -b` 全绿
- `cargo test`（music-gift）全绿
- agent-browser 三视口截图走查：桌面 1280（两 tab + edit 模式 + 选区改稿全链路）、移动 390（dock/sheet）——对照 §10.10 验收清单 9 条逐项过
- `grep -cE '#[0-9a-fA-F]{3,8}\b' frontend/src/styles.css` 不增（仍只在 token 区）
