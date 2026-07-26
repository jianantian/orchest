# v0.14 PRD: Skill 机制(Skill Mechanism)

## 背景

SDK 优化计划主题 D(`docs/todo/2026-07-18-sdk-optimization-plan.md`)。Skill-first 是 polaris 核心判断——"对齐 Anthropic Agent Skills 开放标准,把渐进式披露作为一等抽象"(`docs/polaris/overview.md:17`;2026-07-18 确认设计目标为**非常易用**)。但当前实现与该原则存在系统性落差:

- skill 元数据从不出现在模型视野,纯知识 skill 靠应用手写路径 + 模型自觉 `read_file`;
- 解析失败完全静默(`.ok()?` 吞掉),skill 无声消失;
- 一个坏 skill 让整个 run `RunFailed`;
- 官方标准字段 `allowed-tools`(连字符)被静默忽略,name/description 无校验;
- skill name 未校验直接拼缓存路径(路径注入)。

本迭代清偿,使"放好 SKILL.md 目录、设 `skills_dir` 即可用"成立,并为 demo 的 prompt skill 化(review/music_prompt/countdown,optimization-plan D7)提供前提。

## 目标

1. **零配置披露**: 使用方设 `skills_dir` 后,模型自动知晓可用 skill 列表并可按需加载正文/资源——无需手写路径、无需注册工具、无 CWD 依赖。
2. **失败可见**: 解析/注册失败有结构化警告,不再静默消失;单个坏 skill 不拖垮整个 run。
3. **标准兼容**: 官方字段(`allowed-tools` 连字符、name/description 规范)正确解析与校验。
4. **安全**: skill name 无法逃逸 skill-envs 目录。

## 非目标

- capabilities 的 network/filesystem/max_memory 执行控制(005 只做"兑现或显式标注预留"的裁定,不实现 sandbox)。
- bundled_tools 流程改动(现状可用)。
- demo 侧 prompt skill 化(optimization-plan D7,本迭代之后单独跟进)。

## Issue 分解

| Issue | 标题 | 来源 |
|-------|------|------|
| 001 | skill scanner 错误上报 + frontmatter 加固 | SDK-D2 |
| 002 | 零配置渐进式披露(元数据注入 + load_skill) | SDK-D1 |
| 003 | 注册容错 + 一致性小修 | SDK-D3/D7 |
| 004 | 标准兼容 + name 校验 + 路径注入防护 | SDK-D4/D6 |
| 005 | 死字段/死变体裁定 | SDK-D5 |

依赖顺序: 001 → 002 → 003 → 004 → 005(002 依赖 001 的解析/警告产物;003-005 相互独立,按编号提交)。

## 验收

- 五项检查全绿(test / clippy -D warnings / fmt / lint-check / cargo doc)
- 各 issue spec 验收框全勾
- 易用性硬指标(002): 全新使用方只放 SKILL.md 目录 + 设 `skills_dir`,模型即可知晓并加载 skill——无需手写路径、无需注册工具、无 CWD 依赖

## 依赖

- v0.13(生成质量地基)合入后开工,无文件级耦合
- music-gift 去拍平(D6)与本迭代无依赖,可并行
