use orchest_protocol::{DecisionAnswer, DecisionRequest, DecisionResponse, ErrorCode};
use serde_json::{json, Value};

fn request() -> DecisionRequest {
    serde_json::from_value(json!({"state": {"message":"help"}, "questions": {
        "urgent": {"type":"boolean", "instructions":{"question":"Urgent?"}},
        "team": {"type":"choice", "instructions":"Which team?", "criteria":{"billing":null,"tech":{"about":"bugs"}}},
        "anger": {"type":"score", "instructions":["How angry?"], "criteria":["Calm",{"level":"Frustrated"},"Angry"]}
    }})).unwrap()
}

fn response() -> Value {
    json!({"model":"local/test", "answers":{
        "urgent":{"type":"boolean","probability":0.95},
        "team":{"type":"choice","choice":"billing","probabilities":{"billing":0.9,"tech":0.1},"confidence":0.8},
        "anger":{"type":"score","score":1.05,"probabilities":{"0":0.0,"1":0.95,"2":0.05},"legend":{"0":"Calm","1":{"level":"Frustrated"},"2":"Angry"}}
    }})
}

#[test]
fn portable_contract_preserves_structure_and_missing_metadata() {
    let mut req = request();
    req.state = json!(42); // Generic capability is not restricted by one HTTP API.
    req.validate().unwrap();
    let res: DecisionResponse = serde_json::from_value(response()).unwrap();
    res.validate_for(&req).unwrap();
    assert!(res.usage.is_none());
    assert!(matches!(
        res.answers["urgent"],
        DecisionAnswer::Boolean { probability: 0.95 }
    ));
    let wire = serde_json::to_value(&res).unwrap();
    assert!(wire.get("usage").is_none());
    assert!(wire["answers"]["anger"].get("confidence").is_none());
    assert_eq!(
        wire["answers"]["anger"]["legend"]["1"],
        json!({"level":"Frustrated"})
    );
    assert!(!wire.to_string().contains("noul"));
}

#[test]
fn invalid_questions_fail_before_execution() {
    for question in [
        json!({"type":"boolean","instructions":false}),
        json!({"type":"boolean","instructions":"?","criteria":{"true":false,"false":"no"}}),
        json!({"type":"choice","instructions":"?","criteria":{}}),
        json!({"type":"choice","instructions":"?","criteria":{"a":3}}),
        json!({"type":"score","instructions":"?","criteria":[]}),
        json!({"type":"score","instructions":"?","criteria":[null]}),
    ] {
        let req: DecisionRequest =
            serde_json::from_value(json!({"state":null,"questions":{"q":question}})).unwrap();
        assert_eq!(req.validate().unwrap_err().code, ErrorCode::InvalidRequest);
    }
    let mut req = request();
    req.questions.clear();
    assert!(req.validate().is_err());
}

#[test]
fn incomplete_or_inconsistent_answers_fail_loudly() {
    let paths = [
        ("/answers/urgent/probability", json!(1.1)),
        ("/answers/urgent/type", json!("choice")),
        ("/answers/team/choice", json!("unknown")),
        ("/answers/team/confidence", json!(-0.1)),
        ("/answers/team/probabilities", json!({"billing":1.0})),
        (
            "/answers/team/probabilities",
            json!({"billing":0.1,"tech":0.1}),
        ),
        ("/answers/anger/score", json!(3.0)),
        ("/answers/anger/probabilities", json!({"1":1.0})),
        ("/answers/anger/legend", json!({"0":"Calm"})),
    ];
    for (path, value) in paths {
        let mut res = response();
        *res.pointer_mut(path).unwrap() = value;
        if let Ok(res) = serde_json::from_value::<DecisionResponse>(res) {
            assert_eq!(
                res.validate_for(&request()).unwrap_err().code,
                ErrorCode::InvalidResponse,
                "{path}"
            );
        }
    }
    let mut res: DecisionResponse = serde_json::from_value(response()).unwrap();
    res.answers.remove("team");
    assert!(res.validate_for(&request()).is_err());
    res.answers
        .insert("other".into(), DecisionAnswer::Boolean { probability: 0.2 });
    assert!(res.validate_for(&request()).is_err());
}

#[test]
fn optional_distributions_are_not_invented() {
    let mut wire = response();
    wire["answers"]["team"] = json!({"type":"choice","choice":"tech"});
    wire["answers"]["anger"] = json!({"type":"score","score":0.5});
    let res: DecisionResponse = serde_json::from_value(wire).unwrap();
    res.validate_for(&request()).unwrap();
    let serialized = serde_json::to_value(res).unwrap();
    assert_eq!(
        serialized["answers"]["team"],
        json!({"type":"choice","choice":"tech"})
    );
}
