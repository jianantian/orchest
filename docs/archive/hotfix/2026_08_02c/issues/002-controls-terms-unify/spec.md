# 002 — 按钮/选项控件与术语收敛

## 背景

控件样式方言(现状清单,以 `styles.css` 为准):

- **按钮**:`.btn-primary`(金胶囊)/`.btn-secondary`(描边)是既有正经体系;另有 `.mine-btn`(幽灵文字)、`.owner-edit`、`.version-load`、`.btn-ghost`(小幽灵)、`.chat-send`(金圆钮)、`.undo-btn`/`.restart-btn`/`.suggest-refresh`(三种圆图标)各自为政;
- **选项/chip**:`.suggest-chip`(中性描边 pill)、`.style-chip`(金 pill)、`.gender-opt`(描边圆角矩形)、guided `.pills-row` 的选项 pill、`.version-pill`、`.mode-btn`(chunky 分段控件)——同一"从若干选项中选一个"的语义五六种长相;
- **术语**:tab "Free"、面板 "AI Partner"、文档"创作室"三个名字。

## 目标/范围

`examples/demo/music-gift/frontend/src/`,样式 + class 替换 + i18n 文案,不改组件逻辑。

1. **按钮三档收敛**:
   - 主操作 → `.btn-primary`(金胶囊):创建/生成、保存并重新生成;
   - 次操作 → `.btn-secondary`(描边):保存(标题轻编辑)、owner 区的"编辑"、版本"载入到草稿";
   - 幽灵文字 → 统一一个 `.btn-ghost` 语义(合并 `.mine-btn` 的视觉);危险操作保留 danger 色;
   - 圆图标按钮统一为一种(沿用 `.suggest-refresh` 的 28px 圆形描边幽灵):undo、restart、refresh 同式;
   - `.chat-send` 金圆钮保留(两个聊天面板共用,本身一致)。
2. **选项控件统一为一种 pill 语言**(中性描边 → 金色激活):
   - `.suggest-chip` / guided `.pills-row` pill / `.version-pill` / `.gender-opt` 视觉统一(圆角 100px、描边、hover/激活金);尺寸保留默认/小两档;
   - `.style-chip`(已选风格)保持金色填充 pill(它是"已选态"的视觉,合理),但圆角/字号与上面拉齐;
   - Vocal/Instrumental 的 `.mode-bar` 分段控件拆除,改为两个选项 pill(带图标),与全站选项语言一致;删除 `.mode-bar`/`.mode-btn` 死样式。
3. **术语统一**(五个语言包 zh/en/fr/es/ru 全改):
   - tab:`tab_free` 的文案改为 "创作室/Studio"(fr "Studio"、es "Studio"、ru "Студия");
   - AI 面板标题:`studio_ai_tab` 改为 "AI 协作/AI Co-writer"(fr "Co-auteur IA"、es "Coescritor IA"、ru "ИИ-соавтор");
   - key 名不动,只改文案值。
4. 清理收敛后的死 CSS。

## 验收标准

- [ ] 全站主操作都是金胶囊、次操作都是描边、辅助都是幽灵文字;无第五种按钮
- [ ] 所有"选项"控件(建议 chips、guided 选项、性别、版本、Vocal/Instrumental)同一种 pill 语言,激活态金色一致
- [ ] undo/restart/refresh 三个圆图标按钮样式一致
- [ ] tab 显示"创作室/Studio",AI 面板标题"AI 协作/AI Co-writer",五语言均有译文
- [ ] `.mode-bar`/`.mode-btn` 等被替换的样式已删除,无残留引用
- [ ] 浅色/深色各抽一页无破版;`npm run build` 通过

## Notes

- 圆角/间距以现有 pill(`suggest-chip`)为基准值,不新造令牌。
- guided 的 pills 有 icon+label 结构(如 relationship 选项),统一时保留其内容结构只统一视觉。

## 实施步骤(plan)

读:

- `styles.css` 按钮/chip 相关段(`.btn*` 323-361、`.tab-*` 830-845、`.mode-*` 847-852、`.gender-*` 875-877、`.style-chip*`/`.suggest-*` 918-929、`.mine-btn` 804-811、`.owner-edit`/`.version-*`/`.undo-btn`/`.open-studio-btn` 在文件尾部 studio/edit 段)
- 引用这些 class 的组件:`Studio.tsx`、`GuidedFlow.tsx`、`ChatUI.tsx`(PillsRow/GoldPill)、`GiftPage.tsx`、`MyGiftsPage.tsx`、`ReviewCard.tsx`

改:

1. `styles.css`:定义统一 pill 与圆图标按钮样式;逐 class 替换;删除死样式。
2. 组件 class 名替换(mode-bar → pill 组等),逻辑不动。
3. `i18n.tsx` 五个 dict 改 `tab_free`、`studio_ai_tab` 文案。
4. `npm run build`;agent-browser 截图抽查(guided 选项屏、studio、gift 页 owner 区、mine 列表)+ 深色一页。
