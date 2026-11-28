#![cfg(feature = "http")]

use orchest_protocol::Capability;
use orchest_provider::catalog::{find_model, find_model_for, list_models, ModelFilter};

#[test]
fn projected_chat_rows_have_non_empty_descriptions() {
    let rows: Vec<_> = list_models(ModelFilter {
        capability: Some(Capability::Chat),
        include_deprecated: true,
        ..Default::default()
    })
    .collect();
    assert!(!rows.is_empty());
    for r in rows {
        assert!(
            !r.description.trim().is_empty(),
            "empty description for {}",
            r.id
        );
    }
}

#[test]
fn chat_ids_are_unique_by_capability_provider_model() {
    use std::collections::HashSet;
    let mut seen = HashSet::new();
    for r in list_models(ModelFilter {
        include_deprecated: true,
        ..Default::default()
    }) {
        assert!(
            seen.insert((r.capability, r.provider, r.model)),
            "duplicate {:?}",
            (r.capability, r.provider, r.model)
        );
    }
}

#[test]
fn find_model_peels_openai_responses_protocol() {
    let hit = find_model("openai/responses/gpt-5.4")
        .expect("protocol-explicit id should resolve via ADR-0002");
    assert_eq!(hit.provider, "openai");
    assert_eq!(hit.model, "gpt-5.4");
    assert_eq!(hit.capability, Capability::Chat);
}

#[test]
fn find_model_for_disambiguates_capability() {
    // After Batch 1 Asr rows land this matters more; for Batch 0 assert Chat path.
    let hit = find_model_for("openai/gpt-5.4", Capability::Chat).expect("chat row");
    assert_eq!(hit.capability, Capability::Chat);
}

#[cfg(all(feature = "http", feature = "stream"))]
#[test]
fn discovery_lists_aliyun_streaming_asr_with_descriptions() {
    let rows: Vec<_> = list_models(ModelFilter {
        capability: Some(Capability::Asr),
        provider: Some("aliyun"),
        streaming: Some(true),
        ..Default::default()
    })
    .collect();
    // the deprecated Fun-ASR Realtime row is hidden from default discovery
    assert!(rows.iter().all(|r| r.model != "fun-asr-realtime"));
    assert!(rows
        .iter()
        .any(|r| r.model == "qwen-audio-3.0-asr-flash-streaming"));
    let all: Vec<_> = list_models(ModelFilter {
        capability: Some(Capability::Asr),
        provider: Some("aliyun"),
        streaming: Some(true),
        include_deprecated: true,
        ..Default::default()
    })
    .collect();
    assert!(all.len() >= 2);
    for r in rows.iter().chain(&all) {
        assert!(!r.description.trim().is_empty());
    }
}

#[cfg(feature = "stream")]
#[test]
fn voice_catalog_and_registry_offer_the_same_capabilities() {
    use orchest_protocol::{AsrCapability, CapabilityExt, CapabilitySource};
    use orchest_provider::Registry;

    let registry = Registry::with_builtin();
    let filters = [
        AsrCapability::CommittedAppend,
        AsrCapability::ClientFinalize,
    ];
    let from_catalog: Vec<_> = list_models(ModelFilter {
        asr_capabilities: filters.to_vec(),
        ..Default::default()
    })
    .map(|row| (row.provider, row.model))
    .collect();
    let from_registry: Vec<_> = registry
        .asr()
        .asr_capabilities(filters)
        .list()
        .into_iter()
        .map(|entry| {
            (
                entry.descriptor.provider.as_ref(),
                entry.descriptor.model.as_ref(),
            )
        })
        .collect();
    assert_eq!(from_catalog, from_registry);
    assert_eq!(
        from_catalog,
        [("deepgram", "nova-3"), ("soniox", "stt-rt-v5")]
    );
    let speaker = registry
        .asr()
        .asr_capabilities([AsrCapability::Speaker])
        .select()
        .unwrap();
    assert_eq!(speaker.descriptor.provider, "assemblyai");
    assert_eq!(speaker.descriptor.source, CapabilitySource::Static);

    let tts = registry.tts().streaming().list();
    assert_eq!(tts.len(), 3);
    assert!(registry.tts().bidirectional().list().is_empty());
    for entry in tts {
        let descriptor = &entry.descriptor;
        let id = format!("{}/{}", descriptor.provider, descriptor.model);
        let row = find_model_for(&id, Capability::Tts).unwrap();
        assert!(row.streaming);
        assert!(!row.duplex);
        let CapabilityExt::Tts(ext) = &descriptor.ext else {
            panic!("missing TTS capability details for {id}");
        };
        assert_eq!(ext["source"], "direct");
        assert!(ext["protocol"]
            .as_str()
            .is_some_and(|name| !name.is_empty()));
        assert_eq!(
            serde_json::to_value(row.to_descriptor().ext).unwrap(),
            serde_json::to_value(&descriptor.ext).unwrap()
        );
    }
}

#[cfg(feature = "stream")]
#[tokio::test]
async fn voice_capabilities_are_checked_before_connecting() {
    use std::time::Duration;

    use orchest_protocol::{AudioFormat, ErrorCode, StreamingTranscribeRequest, SynthesizeRequest};
    use orchest_provider::{ProviderConfig, Registry};
    use serde_json::json;
    use tokio::net::TcpListener;
    use tokio::time::timeout;

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let registry = Registry::with_builtin();
    let config = ProviderConfig {
        api_key: Some("test-key".into()),
        api_url: Some(format!("wss://{}", listener.local_addr().unwrap())),
        ..Default::default()
    };
    let adapter = registry.asr().id("deepgram/nova-3").build(&config).unwrap();
    let request = |options| StreamingTranscribeRequest {
        format: AudioFormat::Pcm16Le,
        language: None,
        options,
    };

    let error = adapter
        .start_stream(request(json!({"require_capabilities": ["speaker"]})))
        .await
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::UnsupportedOption);

    let tts = registry
        .tts()
        .id("minimax/speech-2.8-hd")
        .build(&config)
        .unwrap();
    let error = tts
        .stream_synthesize(SynthesizeRequest {
            text: "hello world".into(),
            voice: Some("test-voice".into()),
            format: AudioFormat::Mp3,
            options: json!({"require_capabilities": ["words"]}),
        })
        .await
        .expect_err("TTS word alignment is unavailable");
    assert_eq!(error.code, ErrorCode::UnsupportedOption);

    assert!(timeout(Duration::from_millis(30), listener.accept())
        .await
        .is_err());

    let supported = tokio::spawn(async move {
        adapter
            .start_stream(request(
                json!({"require_capabilities": ["client_finalize"]}),
            ))
            .await
    });
    let (socket, _) = timeout(Duration::from_secs(5), listener.accept())
        .await
        .expect("a supported request should reach the transport")
        .unwrap();
    drop(socket);
    assert_eq!(
        supported.await.unwrap().unwrap_err().code,
        ErrorCode::ProviderStreamError
    );
}
