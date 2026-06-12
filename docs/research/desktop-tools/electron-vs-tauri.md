# Electron vs Tauri 技术选型复审

> 2026-06-12 | 触发：ADR-001 未决问题 1 的调研——Claude Code 桌面端的 browser preview 即「Electron 内嵌 Chromium + CDP」架构
> 性质：对 `multivac-reconstruction-analysis.md` §6.2「为什么 Tauri 而不是 Electron」的正式复审
> 上游：[ADR-001](../../analysis/adr-001-product-positioning.md)（五个面、browser 面为招牌、人读审指 agent 写）

---

## 结论

**推荐改选 Electron**（WebContentsView + `webContents.debugger`），除非 spike 证明「Tauri + sidecar Chromium」的嵌入体验可接受且团队愿意承担其工程复杂度。

原 Tauri 决策的三条论据,在 ADR-001 定位下逐条复检的结果:

| 原论据（§6.2） | 复检结果 |
|---------------|---------|
| Electron 需要 napi-rs FFI 桥接 Rust core | **失效**。前端经 HTTP/WS 连 Rust daemon,壳与 core 进程解耦——两种壳都不需要 FFI,`multivac-core` 一行不改 |
| 二进制 ~5MB vs ~200MB | **权重大幅下降**。目标用户(开发者/创作者)的参照物 VS Code/Slack/Claude/Cursor 全是 Electron;且若走 Tauri + sidecar Chromium,体积优势归零——Chromium 还是要分发或依赖用户安装 |
| Rust「天然一体」 | **部分成立但代价转移**。省下的是壳的语言切换,换来的是系统 webview 碎片化——而碎片化恰好砸在我们五个面里最重的三个上(见下) |

决定性的新因素:**browser 面的感知深度是产品招牌**(ADR-001),而这恰好是两种壳差距最大的地方。

---

## 一、评估框架:五个面对壳的要求

选型不该抽象比较,应对照 ADR-001 的五个 surface 逐个检验:

| Surface | 对壳的关键要求 | Tauri(系统 webview) | Electron(Chromium) |
|---------|--------------|---------------------|---------------------|
| 对话面(TurnCard) | markdown/代码高亮渲染 | ✅ 可 | ✅ 可 |
| 文件/多媒体预览面 | 视频编解码、PDF、图片 | ⚠️ 随系统 webview 漂移;WKWebView 不支持 H.265,版本随用户 macOS 升级而变 | ✅ Chromium 版本锁定随 app 发布,ffmpeg 含专有编解码,行为双平台一致 |
| 终端面(xterm.js) | canvas/WebGL 高频渲染 | ⚠️ xterm.js 在 WKWebView 有已知兼容问题(UA 检测);WKWebView canvas 渲染抖动有公开报告 | ✅ xterm.js 的主场(VS Code 即此组合) |
| git/diff 面 | 文本渲染 | ✅ 可 | ✅ 可 |
| **browser 面(招牌)** | 嵌入第二个可感知的页面 | ❌ 见下,双平台能力不对称且多 webview 仍 unstable | ✅ WebContentsView + 全量 CDP,Claude Desktop 已验证 |

**结论先行:碎片化风险集中在多媒体预览、终端、browser 三个面——恰好是创作工作台最重的三个面。**

---

## 二、browser 面:决定性差异的细节

### 2.1 Electron 侧

- **WebContentsView**(Electron v29+,BrowserView 的正式后继):由 main process 创建、定位、分层,专为「同一窗口内嵌入多个页面」设计——browser 面就是一个 WebContentsView,与 React UI 并排
- **`webContents.debugger`**:对任意 webContents attach Chrome DevTools Protocol——截图(`Page.captureScreenshot`)、DOM(`DOM.getDocument` / `DOM.getNodeForLocation`,即元素级 deixis)、console(`Runtime.consoleAPICalled`)、网络(Network 域)、输入注入(Input 域,agent 反向操作)全部原生
- **已被验证**:Claude Code 桌面端的 preview 面板(autoVerify 截图/查 DOM/点元素/填表单)就是这套架构跑在生产里

### 2.2 Tauri 侧

- **多 webview 仍在 unstable feature flag**:官方 PR #8280 合入后至今藏在 unstable cargo feature 后面,API 未定稿;社区已报告多 webview 白屏(#10011)、z-order 覆盖主 webview 等问题——而 Surface 区恰恰需要稳定的同窗多 webview
- **感知能力双平台不对称**:
  - Windows/WebView2:有官方 CDP(`CallDevToolsProtocolMethod`,或 `--remote-debugging-port` 给 Playwright 连)——可用
  - **macOS/WKWebView:没有任何官方编程接口**。Apple 的自动化故事只有 safaridriver(只管 Safari 本体);嵌入式 WKWebView 无 WebDriver、无 CDP,社区只能自建 bridge(2026 年仍有人为此专门造轮子)
  - 即:Tauri 路线 A 在我们的主力开发平台(macOS)上感知是空的
- **逃逸路线 = sidecar Chromium(路线 B)**:Tauri 壳 + 独立 Chromium 进程走 CDP,嵌入靠 `Page.startScreencast` 帧流回贴 + Input 域转发输入。感知满血,但:体积优势归零、新增帧流/输入转发/进程生命周期三块自研工程、嵌入体验上限低于原生 view

---

## 三、自有 UI 的一致性:被低估的第二战场

即使不考虑 browser 面,系统 webview 的版本漂移对**我们自己的 UI**也是真实成本:

- 用户机器上的 WKWebView/WebView2 版本不受我们控制(随 OS 更新),同一份前端代码在不同用户机器上行为不同——bug 报告将带上「你的 macOS 版本是多少」这层排查维度
- Electron 把 Chromium 钉死在 app 版本里:我们测过的渲染行为就是用户看到的行为
- 对低交互的表单类应用这无所谓;对一个以**终端、视频预览、高密度 canvas**为核心面的创作工作台,这是质量地板的差异

---

## 四、其余维度速查

| 维度 | Tauri | Electron | 对我们的权重 |
|------|-------|----------|------------|
| 安装包/磁盘 | ~5-10MB | ~100-200MB | 低(开发者/创作者人群) |
| 内存基线 | 较低 | 较高 | 低(跑 dev server + agent 时,壳不是大头) |
| 安全模型 | capability/ACL 较严 | 需自律(contextIsolation、sandbox、禁 nodeIntegration) | 中——但我们的壳极薄(renderer 只连 daemon 的 WS),攻击面可控 |
| 自动更新 | tauri-updater | electron-updater/Squirrel | 持平 |
| Linux | 支持(webkitgtk,质量一般) | 支持(成熟) | 低(v0 不承诺;Claude Desktop 也不出 Linux) |
| 团队技能 | Rust(有)+ 壳 API 学习 | TS/Node(有) | 持平 |
| 同类产品参照 | **零**(我们研究过的同类无一用 Tauri) | Craft Agents、Nimbalyst、Claude Desktop、Cursor(VS Code fork) | 高——成熟坑都被踩过 |

## 五、Rust core 不受影响(选型的可逆性边界)

两种壳下 `multivac-core` 完全相同:Rust daemon 进程,axum HTTP/WS,RuntimeBackend,SQLite。壳只负责:

```
Electron 方案:
  main process(薄):spawn multivac daemon → 创建 BaseWindow
    ├── WebContentsView #1:React UI(连 daemon WS)
    └── WebContentsView #2:browser 面(daemon 经 main 的 debugger API 感知)
```

**可逆性边界**:壳与 core 解耦使「换壳」不伤 Rust 资产;但 surface 适配器(browser 面的感知适配器、终端面的渲染宿主)有壳相关实现——这部分代码写在哪个壳上,迁移就要重写。所以这个决策要在阶段 1(介质面)动工前定死,不能拖到阶段 2。

---

## 六、决策建议与 spike 任务

**建议:Electron。** 理由收敛为一句话:在 ADR-001 的定位下,壳的核心职责从「包一个网页」变成了「承载五个创作介质面」,其中三个面(多媒体/终端/browser)对渲染一致性和 CDP 有硬需求——这正是 Electron 的本职和 Tauri 的软肋,且原选 Tauri 的首要论据(FFI)已被 daemon 架构消解。

若团队仍倾向 Tauri,先跑路线 B 的 spike 再定(预算 1-2 天):

1. Tauri 壳内以 screencast 嵌入 sidecar Chromium:测帧率、滚动跟手度、resize 行为
2. Input 域转发鼠标/键盘:测点击精度与中文输入法
3. `DOM.getNodeForLocation` 元素级 deixis:点击 → 元素引用链路打通
4. 资源对照:Tauri+sidecar vs Electron 的实际磁盘/内存差(预期:差距远小于 §6.2 原表)

同时无论选哪边,都补一个半天 spike:xterm.js + 大输出滚动 + 视频(H.264/H.265)预览在目标壳上的实测——把「一致性」从论据变成数据。

---

## 参考

- Claude Code Desktop 文档(preview 面板/pane 布局/元素选取):https://code.claude.com/docs/en/desktop
- Electron WebContentsView:https://www.electronjs.org/docs/latest/api/web-contents-view
- Electron Debugger(CDP):https://www.electronjs.org/docs/latest/api/debugger
- Tauri 多 webview PR(unstable):https://github.com/tauri-apps/tauri/pull/8280
- Tauri 多 webview 白屏 issue:https://github.com/tauri-apps/tauri/issues/10011
- WebView2 CDP 官方文档:https://learn.microsoft.com/en-us/microsoft-edge/webview2/how-to/chromium-devtools-protocol
- WKWebView 无官方 WebDriver(社区自建桥):https://danielraffel.me/2026/02/14/i-built-a-webdriver-for-wkwebview-tauri-apps-on-macos/
- xterm.js 在 WKWebView 的问题:https://github.com/xtermjs/xterm.js/issues/3575
- Tauri H.265 支持缺失:https://github.com/tauri-apps/tauri/issues/11559
- Tauri webview 版本漂移说明:https://v2.tauri.app/reference/webview-versions/
