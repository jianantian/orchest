// PROTOTYPE — throwaway, delete after Studio workbench design is approved.
// Question: 创作页宽工作台的 IA 与核心交互是否成立？
// Route: /prototype/studio?view=studio|guided|mobile (dev-only, see App.tsx)
// Static mock, real semantic tokens (post-Phase-1). Same data in every view.
// v2: 单一卡片语言统一两栏；移动端双机并列（编辑态 / 试听与AI）。
import { useEffect } from "react";
import { useSearchParams } from "react-router-dom";
import { PrototypeSwitcher, type ProtoView } from "../components/PrototypeSwitcher";
import "../styles-prototype.css";

const VIEWS: ProtoView[] = [
  { key: "studio", name: "创作室 · 桌面双栏" },
  { key: "guided", name: "引导模式 · 桌面双栏" },
  { key: "mobile", name: "创作室 · 移动（编辑 / 试听AI）" },
];

const LYRICS_BEFORE = ["巷口的灯亮到第几盏", "你才慢吞吞地走回家"];
const DIFF_OLD = ["我把生日快乐折成纸船", "放进你路过的每一个夏天"];
const DIFF_NEW = ["我把祝福折成一只纸船", "放进你经过的每一个夏天"];
const LYRICS_AFTER = ["愿所有温柔都准时抵达", "像此刻窗外刚好亮起的霞"];

function Topbar({ active }: { active: "guided" | "studio" }) {
  return (
    <div className="pw-topbar">
      <div className="pw-tabs">
        <button className={`pw-tab${active === "guided" ? " on" : ""}`}>引导创作</button>
        <button className={`pw-tab${active === "studio" ? " on" : ""}`}>创作室</button>
      </div>
      <div className="pw-topbar-actions">
        <span className="pw-draft-state">草稿已保存 · 22:41</span>
        {/* undo 是创作室专属（撤销 AI 改稿），引导模式不显示 */}
        {active === "studio" && <button className="pw-icon-btn" title="撤销 AI 修改">↩</button>}
      </div>
    </div>
  );
}

function StyleCard() {
  return (
    <div className="pw-card">
      <div className="pw-section-head"><span className="pw-section-label">风格</span></div>
      <div className="pw-chips">
        <span className="pw-chip">流行 ✕</span>
        <span className="pw-chip">温暖 ✕</span>
        <span className="pw-chip">女声 ✕</span>
        <span className="pw-chip plain">+ 民谣</span>
        <span className="pw-chip plain">+ 钢琴</span>
        <span className="pw-chip plain">↻</span>
      </div>
    </div>
  );
}

function PlayerCard() {
  return (
    <div className="pw-card">
      <div className="pw-player">
        <div className="pw-disc" />
        <div className="pw-player-meta">
          <p className="pw-player-title">给小雨的歌</p>
          <p className="pw-player-sub">V2 · 3:24 · Suno v4</p>
        </div>
        <button className="pw-icon-btn" style={{ width: 36, height: 36, fontSize: 15 }}>▶</button>
      </div>
      <div className="pw-range-wrap">
        <div className="pw-range-bar">
          <div className="pw-range-track" />
          <div className="pw-range-played" />
          <div className="pw-range-sel" />
          <div className="pw-range-handle l" />
          <div className="pw-range-handle r" />
        </div>
        <div className="pw-range-info">
          <span>已选 <b>0:42 – 1:05</b></span>
          <span className="map">→ 对应歌词第 3–4 行</span>
          <button className="pw-btn pw-btn-primary pw-btn-sm" style={{ marginLeft: "auto" }}>交给 AI 修改</button>
        </div>
      </div>
    </div>
  );
}

function TakesCard() {
  return (
    <div className="pw-card pw-takes-card">
      <div className="pw-take on">
        <span className="pw-take-badge">V2</span>
        <div className="pw-take-meta">
          <p className="pw-take-name">当前版本 · 试听中</p>
          <p className="pw-take-sub">副歌情绪加强 · 22:38</p>
        </div>
        <div className="pw-take-actions">
          <button className="pw-icon-btn" title="从这一版分叉">⑂</button>
        </div>
      </div>
      <div className="pw-take">
        <span className="pw-take-badge">V1</span>
        <div className="pw-take-meta">
          <p className="pw-take-name">初版</p>
          <p className="pw-take-sub">22:15</p>
        </div>
        <div className="pw-take-actions">
          <button className="pw-icon-btn" title="试听">▶</button>
          <button className="pw-icon-btn" title="从这一版分叉">⑂</button>
        </div>
      </div>
    </div>
  );
}

function ChatCard() {
  return (
    <div className="pw-card pw-chat-card">
      <div className="pw-section-head"><span className="pw-section-label">AI 协作</span></div>
      <div className="pw-chat">
        <div className="pw-bubble user">副歌再往上推一点情绪，但别改主歌</div>
        <div className="pw-bubble bot">
          我把第 3–4 行的意象从「生日快乐」换成「祝福」，情绪更内敛，韵脚保持「天」不变。提案已放在手稿里，你可以逐段接受。
          <span className="pw-applied">已生成提案 · 歌词第 3–4 行</span>
        </div>
      </div>
      <div className="pw-chatbar">
        <textarea rows={1} placeholder="讨论整体方向，或圈选歌词精准修改…" />
        <button className="pw-send">➤</button>
      </div>
    </div>
  );
}

/* ── 创作室 · 桌面 ── */
function StudioDesktop() {
  return (
    <div className="pw-bench">
      <Topbar active="studio" />
      <div className="pw-cols">
        <div className="pw-col pw-col-driver">
          {/* 手稿卡：标题在纸上，正文可划选，AI 提案行内 diff */}
          <div className="pw-card pw-doc">
            <input className="pw-title-input" defaultValue="给小雨的歌" />
            <hr className="pw-doc-divider" />
            <div className="pw-manuscript">
              <div className="pw-seltoolbar">
                <button>改写</button>
                <button>更押韵</button>
                <button>更口语</button>
                <button>缩短</button>
                <button className="accent">自定义指令…</button>
              </div>
              {LYRICS_BEFORE.map((l) => <p key={l} className="pw-line">{l}</p>)}
              <div className="pw-diff">
                {DIFF_OLD.map((l) => <p key={l} className="pw-diff-old">{l}</p>)}
                {DIFF_NEW.map((l) => <p key={l} className="pw-diff-new">{l}</p>)}
                <div className="pw-diff-actions">
                  <span className="pw-diff-note">AI 提案 · 第 3–4 行</span>
                  <button className="pw-btn pw-btn-primary pw-btn-sm">✓ 接受</button>
                  <button className="pw-btn pw-btn-secondary pw-btn-sm">✕ 拒绝</button>
                </div>
              </div>
              {LYRICS_AFTER.map((l) => <p key={l} className="pw-line">{l}</p>)}
            </div>
          </div>
          <StyleCard />
          <div className="pw-action-row">
            <button className="pw-btn pw-btn-secondary">保存</button>
            <button className="pw-btn pw-btn-primary grow">保存并重新生成</button>
          </div>
        </div>

        <div className="pw-col pw-col-artifact">
          <PlayerCard />
          <TakesCard />
          <ChatCard />
        </div>
      </div>
    </div>
  );
}

/* ── 引导模式 · 桌面 ── */
function GuidedDesktop() {
  return (
    <div className="pw-bench">
      <Topbar active="guided" />
      <div className="pw-cols">
        <div className="pw-col pw-col-driver">
          <div className="pw-card pw-chat-card" style={{ flex: 1 }}>
            <div className="pw-chat">
              <div className="pw-bubble bot">这首歌想送给谁？</div>
              <div className="pw-bubble user">送给闺蜜</div>
              <div className="pw-bubble bot">她叫什么名字？</div>
              <div className="pw-bubble user">小雨</div>
              <div className="pw-bubble bot">想对小雨说什么？随便聊聊，我来把它写成歌。</div>
            </div>
            {/* 引导模式的标志控件：快捷选项 pills 钉在输入条上方 */}
            <div className="pw-quickpills">
              <span className="pw-chip plain">我们十年的回忆</span>
              <span className="pw-chip plain">她总照顾我</span>
              <span className="pw-chip plain">想给她惊喜</span>
            </div>
            <div className="pw-chatbar">
              <textarea rows={1} placeholder="说点什么…" />
              <button className="pw-send">➤</button>
            </div>
          </div>
        </div>

        <div className="pw-col pw-col-artifact">
          <div className="pw-card">
            <div className="pw-section-head"><span className="pw-section-label">需求卡</span></div>
            <dl className="pw-brief">
              <dt>送给</dt><dd>小雨（闺蜜）</dd>
              <dt>场合</dt><dd>生日</dd>
              <dt>氛围</dt><dd>温暖 · 回忆感</dd>
              <dt>人声</dt><dd>女声</dd>
            </dl>
          </div>
          <div className="pw-card">
            <div className="pw-section-head">
              <span className="pw-section-label">歌词审核</span>
              <span className="pw-badge" style={{ marginLeft: "auto" }}>质检通过</span>
            </div>
            <div className="pw-review-snippet">
              巷口的灯亮到第几盏<br />你才慢吞吞地走回家<br />我把生日快乐折成纸船…
            </div>
            <div className="pw-action-row" style={{ marginTop: 14 }}>
              <button className="pw-btn pw-btn-secondary pw-btn-sm">在创作室中打开</button>
              <button className="pw-btn pw-btn-primary pw-btn-sm" style={{ marginLeft: "auto" }}>确认，生成音乐</button>
            </div>
          </div>
          <div className="pw-card">
            <div className="pw-section-head" style={{ marginBottom: 0 }}><span className="pw-section-label">生成进度</span></div>
            <p className="pw-player-sub" style={{ marginTop: 2 }}>正在谱曲与演唱…</p>
            <div className="pw-progress-track"><div className="pw-progress-fill" /></div>
          </div>
        </div>
      </div>
    </div>
  );
}

/* ── 移动 · 编辑态：手稿可编辑、可选区、有生成 CTA ── */
function MobileEditPhone() {
  return (
    <div className="pw-phone">
      <div className="pw-tabs">
        <button className="pw-tab">引导创作</button>
        <button className="pw-tab on">创作室</button>
      </div>
      <div className="pw-phone-body">
        <div className="pw-card pw-doc">
          <input className="pw-title-input" defaultValue="给小雨的歌" style={{ fontSize: 22 }} />
          <hr className="pw-doc-divider" />
          <div className="pw-manuscript">
            {/* 长按划选 → 浮动工具条（紧凑版） */}
            <div className="pw-seltoolbar compact">
              <button>改写</button>
              <button>押韵</button>
              <button>口语</button>
              <button>缩短</button>
              <button className="accent">指令…</button>
            </div>
            {LYRICS_BEFORE.map((l) => <p key={l} className="pw-line">{l}</p>)}
            <p className="pw-line sel" style={{ marginTop: 40 }}>{DIFF_NEW[0]}</p>
            <p className="pw-line sel">{DIFF_NEW[1]}</p>
            {LYRICS_AFTER.map((l) => <p key={l} className="pw-line">{l}</p>)}
          </div>
        </div>
        <div className="pw-card">
          <div className="pw-section-head"><span className="pw-section-label">风格</span></div>
          <div className="pw-chips">
            <span className="pw-chip">流行 ✕</span>
            <span className="pw-chip">温暖 ✕</span>
            <span className="pw-chip plain">+</span>
          </div>
        </div>
        <button className="pw-btn pw-btn-primary" style={{ width: "100%" }}>保存并重新生成</button>
      </div>
      {/* 吸底迷你播放器：滚动手稿音乐不停；⌃ 上拉展开试听 sheet */}
      <div className="pw-dock">
        <div className="pw-disc" />
        <div className="pw-dock-meta">
          <p className="pw-dock-title">给小雨的歌 · V2</p>
          <div className="pw-dock-progress"><i /></div>
        </div>
        <button className="pw-icon-btn">▶</button>
        <button className="pw-icon-btn" title="展开试听与 AI">⌃</button>
      </div>
    </div>
  );
}

/* ── 移动 · 试听与 AI 态：dock 上拉成 sheet ── */
function MobileListenPhone() {
  return (
    <div className="pw-phone">
      <div className="pw-tabs">
        <button className="pw-tab">引导创作</button>
        <button className="pw-tab on">创作室</button>
      </div>
      <div className="pw-phone-body">
        <div className="pw-card pw-doc">
          <input className="pw-title-input" defaultValue="给小雨的歌" style={{ fontSize: 22 }} />
          <hr className="pw-doc-divider" />
          <div className="pw-manuscript">
            {LYRICS_BEFORE.map((l) => <p key={l} className="pw-line">{l}</p>)}
            {DIFF_NEW.map((l) => <p key={l} className="pw-line">{l}</p>)}
            {LYRICS_AFTER.map((l) => <p key={l} className="pw-line">{l}</p>)}
          </div>
        </div>
      </div>
      <div className="pw-dim" />
      <div className="pw-sheet">
        <div className="pw-sheet-grab" />
        <div className="pw-sheet-body">
          <PlayerCard />
          <TakesCard />
          <ChatCard />
        </div>
      </div>
    </div>
  );
}

function MobileView() {
  return (
    <div className="pw-phones">
      <div className="pw-phone-col">
        <MobileEditPhone />
        <span className="pw-phone-label">编辑态 · 划选歌词 → AI 指令</span>
      </div>
      <div className="pw-phone-col">
        <MobileListenPhone />
        <span className="pw-phone-label">试听与 AI · dock 上拉展开</span>
      </div>
    </div>
  );
}

export default function PrototypeStudioPage() {
  const [searchParams] = useSearchParams();
  const view = searchParams.get("view") ?? "studio";
  const cls = VIEWS.some((v) => v.key === view) ? view : "studio";

  useEffect(() => {
    const html = document.documentElement;
    html.classList.add("proto-active");
    return () => html.classList.remove("proto-active");
  }, []);

  return (
    <div className="pw">
      {cls === "studio" && <StudioDesktop />}
      {cls === "guided" && <GuidedDesktop />}
      {cls === "mobile" && <MobileView />}
      {import.meta.env.DEV && <PrototypeSwitcher views={VIEWS} current={cls} />}
    </div>
  );
}
