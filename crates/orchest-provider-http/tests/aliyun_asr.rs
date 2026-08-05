use bytes::Bytes;
use orchest_protocol::{AudioFormat, ErrorCode, Language, TranscribeRequest};
use orchest_provider_http::asr::aliyun::{build_request_body, parse_response};
use serde_json::json;

fn request(format: AudioFormat) -> TranscribeRequest {
    TranscribeRequest {
        audio: Bytes::from_static(b"pcm"),
        format,
        language: Some(Language("zh".into())),
        options: json!({
            "sample_rate": 16000,
            "vocabulary_id": "vocab-1",
            "vocabulary": {"Orchest": 5},
            "context": [
                {"role": "user", "text": "Murmur"},
                {"role": "assistant", "text": "好的"}
            ]
        }),
    }
}

#[test]
fn builds_exact_m4a_request_shape() {
    let body = build_request_body("qwen-audio-3.0-asr-flash", &request(AudioFormat::M4a))
        .expect("request body");

    assert_eq!(body["model"], "qwen-audio-3.0-asr-flash");
    assert_eq!(body["parameters"]["format"], "m4a");
    assert_eq!(body["parameters"]["sample_rate"], "16000");
    assert_eq!(body["parameters"]["language_hints"], json!(["zh"]));
    assert_eq!(body["parameters"]["vocabulary_id"], "vocab-1");
    assert_eq!(body["parameters"]["vocabulary"]["Orchest"], 5);
    assert_eq!(body["input"]["messages"][0]["role"], "user");
    assert_eq!(
        body["input"]["messages"][0]["content"][0]["type"],
        "input_text"
    );
    assert_eq!(body["input"]["messages"][1]["role"], "assistant");
    assert_eq!(
        body["input"]["messages"][2]["content"][0]["type"],
        "input_audio"
    );
    assert_eq!(
        body["input"]["messages"][2]["content"][0]["input_audio"]["data"],
        "data:audio/mp4;base64,cGNt"
    );
}

#[test]
fn aac_uses_aac_mime_and_http_sample_rate_is_always_string() {
    let mut req = request(AudioFormat::Aac);
    req.options["sample_rate"] = json!(48_000);
    let body = build_request_body("model", &req).expect("request body");

    assert_eq!(body["parameters"]["sample_rate"], "48000");
    assert_eq!(
        body["input"]["messages"][2]["content"][0]["input_audio"]["data"],
        "data:audio/aac;base64,cGNt"
    );
}

#[test]
fn parses_text_and_preserves_diagnostics() {
    let result = parse_response(json!({
        "output": {
            "text": "你好",
            "sentence": {"sentence_id": 1, "sentence_end": true, "text": "你好"}
        },
        "usage": {"duration": 2},
        "request_id": "req-1"
    }))
    .expect("response");

    assert_eq!(result.text, "你好");
    assert_eq!(result.diagnostic_metadata["request_id"], "req-1");
    assert_eq!(result.diagnostic_metadata["usage"]["duration"], 2);
    assert_eq!(result.diagnostic_metadata["sentence"]["sentence_id"], 1);
}

#[test]
fn missing_output_text_is_a_protocol_error() {
    let error =
        parse_response(json!({"output": {}, "request_id": "req-1"})).expect_err("missing text");

    assert_eq!(error.code, ErrorCode::ProviderHttpError);
}
