# TODO: 对象存储统一抽象(Object Storage)

> 状态: **部分实现** —— OSS + COS 第一波已落地(独立 crate `orchest-storage`,
> 含预签名 GET URL,2026-08-19);TOS 第二批;motif 集成待下游仓库执行 |
> 需求方: motif(下游产品) | 记录于 2026-08-18
> 这不是 AI provider 能力,是资产持久化基建。v0.9.12 统一路线的 Step 3 设想里已提过
> "storage/asset 持久化"应收进 core(见 [`provider-unification.md`](./provider-unification.md) Step 3),
> 本文档把这条落成具体需求。

## 背景:为什么需要这层抽象

motif(AI 音乐礼物产品)生成的资产——音频、封面图、用户照片、生成的 countdown HTML——
必须持久化到对象存储。驱动这个需求的两个真实证据:

1. **provider 临时 URL 过期(线上事故)**。音乐 provider(sunoapi)返回的资产 URL 托管在
   临时存储上,几天到几周后 404。motif 曾把这些短命 URL 直接写进 DB,导致存量歌曲全部
   坏死。生成完成时下载转存到自己的对象存储是唯一根治路径。
2. **多厂商需求已出现**。motif 现网用阿里云 OSS;仓库里已有一个手搓的腾讯 COS 客户端
   (`motif/src/cos.rs`,HMAC-SHA1 q-sign-algorithm 签名,可直接捐献为参考实现);
   火山 TOS 是可预见的第三个。每家签名方案不同,但消费契约完全一样——这正是
   orchest provider 墙处理过的问题形态。

如果每个下游产品各自手搓 N 家对象存储签名,就是 provider-unification 之前的
"多份 hex/签名重复"在资产层的重演。

## 消费契约(motif 侧的真实需求,验收以此为准)

```rust
/// 统一对象存储接口。async-trait(FFI 兼容,见 CONVENTIONS)。
#[async_trait]
pub trait ObjectStore: Send + Sync {
    /// 上传整个对象。调用方负责尺寸上限(motif 侧 20 MiB),接口吃 `Vec<u8>`
    /// 不做流式(见非目标)。
    async fn put_object(&self, key: &str, bytes: Vec<u8>, content_type: &str) -> Result<(), ObjectStoreError>;
    /// 读回整个对象,直连源站(vendor endpoint)——不经 CDN/公开域名,
    /// 供服务端代理场景拿到未过缓存的新鲜内容。
    async fn get_object(&self, key: &str) -> Result<Vec<u8>, ObjectStoreError>;
    /// 删除对象;删不存在的 key 不算错(幂等)。
    async fn delete_object(&self, key: &str) -> Result<(), ObjectStoreError>;
    /// 预签名 GET URL,`ttl` 内有效(链接即能力;对象本身保持非公开)。
    /// 纯本地计算,不发网络请求。
    fn presigned_url(&self, key: &str, ttl: std::time::Duration) -> String;
    /// 对象的公开访问 URL(CDN/公共读域名拼接,不签名)。
    fn public_url(&self, key: &str) -> String;
    /// 公开 URL 前缀,供调用方从已存 URL 反推 key(删除清理路径用)。
    fn public_base(&self) -> &str;
}
```

工厂按身份选择方言,与 provider registry 的选择模式一致:

```rust
pub enum ObjectStoreDialect { AliyunOss, TencentCos, VolcengineTos }

pub struct ObjectStoreConfig {
    pub dialect: ObjectStoreDialect,
    pub access_key_id: String,
    pub access_key_secret: String,
    pub bucket: String,
    /// 厂商端点(OSS: `oss-cn-hangzhou.aliyuncs.com`;COS: region `ap-guangzhou`;
    /// TOS: `tos-cn-beijing.volces.com`)。统一叫 endpoint,各 dialect 自行拼 host。
    pub endpoint: String,
    /// 公开访问 URL 前缀(CDN 域名或 bucket 公共读域名)。
    pub public_base: String,
}

pub fn create_object_store(config: ObjectStoreConfig) -> Result<Arc<dyn ObjectStore>, ObjectStoreError>;
```

硬性要求(全部来自 motif 现有实现的教训):

- **可注入 transport endpoint 和时钟**。单测用 loopback server 断言签名请求的形状、
  用固定时钟断言签名确定性——motif 的 `CosClient` 已有这套模式(`endpoint` 字段 +
  `now: fn() -> i64`),照搬到每个 dialect。
- **删除幂等**。清理路径会重复删同一 key(当前资产 + 历史版本),404 不得报错。
- **错误类型用 thiserror 独立定义**(`ObjectStoreError`),非 2xx 响应要带 status 和
  body 摘要(截断 ~200 字符),这是排查签名/权限问题的命脉。
- 环境变量读取(`MOTIF_OSS_*` 之类)留在应用层,SDK 只收显式 config。

## 方言签名方案(实现参考)

| dialect | 签名 | 参考 |
|---|---|---|
| 阿里云 OSS | V1:`Authorization: OSS <AK>:<base64(HMAC-SHA1(StringToSign))>`,StringToSign = VERB+MD5+Content-Type+Date+CanonicalizedResource | <https://help.aliyun.com/zh/oss/developer-reference/include-signatures-in-the-authorization-header> |
| 腾讯 COS | V5 `q-sign-algorithm=sha1`(只签 host 头即可覆盖 PUT/DELETE 无 query 场景) | motif `src/cos.rs` 现成实现,直接搬运 |
| 火山 TOS | SigV4 风格 `TOS4-HMAC-SHA256`(sha256 + 派生签名钥) | <https://www.volcengine.com/docs/6349/74830> |

OSS/COS 是第一波(motif 立即消费 OSS;COS 有现成代码);TOS 可第二批,
但 trait 设计时就要按三家验证接口正交性(尤其 TOS 的 SigV4 派生钥流程
是否会顶破 config 形状)。

## 落点(已定:独立 `orchest-storage` crate)

2026-08-19 决定:**独立 `orchest-storage` crate**(初版短暂落在
`orchest-provider-core::storage` + `storage` feature,同日后迁出)。理由:

- **消费方向不同**:storage 的消费者是下游产品(motif),不是任何 provider
  impl crate——core 里它是唯一无人消费的模块;core 的自述是"provider impl
  crates 的 L0/L1 积木"。
- **耦合为零**:整个模块只用到 core 的一行(`shared_client()`);迁出后自持
  reqwest client(同默认值),不依赖 `orchest-protocol` / `orchest-provider-core`。
- **命名冲突**:core 的 `oss` feature(gen-task poller)与 `storage::oss`
  方言是两个不同的 "oss";单立后 `orchest-storage::oss` 无歧义。
- **增长轨迹**:TOS、multipart、list/head/copy 落在这里,独立发版。

接口形状不变:消费契约(trait/config/factory)与本文档一致。

## 非目标(第一期明确不做)

- multipart/分片上传、断点续传
- list/head/copy 等对象管理操作(motif 的清理路径只删已知 key)
- 流式 body(资产 ≤ 20 MiB,内存 `Vec<u8>` 足够)

预签名 URL 已实现(带 TTL 的 GET 签名,OSS query-string 签名 / COS q-sign
query 参数),源自 v0.9.12 前 `agent-runtime-aigc-providers/storage/oss.rs` 的
历史实现(commit 7b8afe5 删除)。motif 现网仍走公共读 + 不可猜测 key;预签名
供将来"查看授权"场景直接使用。
- [x] `get_object` 直连源站读回(签名 GET,不经 public_base/CDN),已知签名
      向量 + loopback 断言;读 404 报错(不幂等),403 带 body 摘要
- [x] `delete_object` 对不存在 key 幂等(loopback 返回 204/404 均 Ok)
- [x] OSS + COS 签名单测:固定时钟下签名输出确定(已知向量,Python 独立实现交叉
      验证);loopback server 断言请求 method/path/Authorization/Content-Type
      形状(TOS 第二批,未实现)
- [x] 非 2xx 错误带 status + body 摘要(截断 ~200 字符)
- [x] `create_object_store` 按 dialect 身份分发:TOS 返回 `UnsupportedDialect`,
      空配置字段返回 `InvalidConfig`
- [ ] motif 集成:删除 `src/cos.rs`,audio/cover/photo/countdown 四条资产路径
      全部经由 `ObjectStore`,存量手搓签名代码清零(motif 仓库不在本仓库,
      需在 motif 侧执行)
- [x] `cargo test --workspace` / `clippy -D warnings` / `fmt --check` 全绿
      (`orchest-storage` 为 workspace member,随 workspace 命令覆盖)

## 约束

按 CONVENTIONS 执行:`async-trait`(不用 `-> impl Future`)、`thiserror`、
库代码禁 `unwrap()`/`expect()`(`#[cfg(test)]` 除外)、无 `unsafe`。
签名依赖(hmac/sha1/md-5/base64/hex)是独立 crate 的固有依赖——消费者以
依赖该 crate 的方式 opt in,无需 feature gate。
