# Issue 004:浅色模式滚动条配色

## 背景

`.app-main` 的滚动条滑块用 `--border`(浅色 `#e4dbd0`)画在奶油底色(`--bg` `#f7f3ec`)上,4px 宽几乎不可见;`scrollbar-gutter: stable` 常驻的 gutter 因此看起来像右侧一条突兀的空白带,用户误以为界面右边没对齐。另外没有 `scrollbar-color`/`scrollbar-width`,Firefox 完全用系统默认滚动条。

## 目标/范围

- 引入独立的滚动条令牌 `--scrollbar-thumb` / `--scrollbar-thumb-hover`,浅色深色系对比(浅色 `rgba(42,31,20,.22)`,深色 `rgba(232,221,208,.25)`)
- 显式声明 track 透明;补 Firefox 的 `scrollbar-width: thin` + `scrollbar-color`
- hover 时滑块加深

非目标:不改滚动条宽度(仍 4px);不动 `scrollbar-gutter` 与 header 的 padding 补偿;其他滚动容器(如 lrc-container 隐藏滚动条)不变。

## 验收标准

- [ ] 浅色模式下滚动条滑块肉眼可辨,gutter 不再像一条"错位的空白边"
- [ ] 深色模式滑块相应提亮,不刺目
- [ ] Firefox 下滚动条为 thin 且配色与 WebKit 一致
- [ ] 滑块 hover 有加深受 feedback

## 实施要点

- `styles.css` `:root` 与 `html[data-theme="dark"]` 各加两个令牌
- `.app-main` 加 `scrollbar-width`/`scrollbar-color`;`::-webkit-scrollbar-track { background: transparent }`;thumb 换令牌并加 `:hover` 规则
