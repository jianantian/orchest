# orchest-storage

Unified object storage for asset persistence in [Orchest](https://github.com/jianantian/orchest)-based
generation pipelines. Generated assets such as audio, images and HTML must
be stored before provider-hosted URLs expire. This crate puts each vendor's
signing dialect behind one async `ObjectStore` trait.

| Dialect | Signature |
| --- | --- |
| Aliyun OSS | V1 header signature (HMAC-SHA1) |
| Tencent COS | V5 `q-sign-algorithm=sha1` |
| Volcengine TOS | planned; rejected by the factory today |

```rust
use orchest_storage::{create_object_store, ObjectStoreConfig, ObjectStoreDialect};

// Inside an async fn returning Result<_, orchest_storage::ObjectStoreError>:
let store = create_object_store(ObjectStoreConfig {
    dialect: ObjectStoreDialect::AliyunOss,
    access_key_id: "AK".into(),
    access_key_secret: "SK".into(),
    bucket: "my-bucket".into(),
    endpoint: "oss-cn-hangzhou.aliyuncs.com".into(),
    public_base: "https://cdn.example.com".into(),
})?;
store.put_object("songs/1.mp3", bytes, "audio/mpeg").await?;
let url = store.presigned_url("songs/1.mp3", std::time::Duration::from_secs(3600));
```

Out of scope: multipart upload, list/head/copy and streaming bodies.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
