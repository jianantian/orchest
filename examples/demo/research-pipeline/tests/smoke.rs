use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("demo manifest must live below the repository root")
        .to_path_buf()
}

#[test]
#[ignore = "requires RESEARCH_PIPELINE_CHAT_MODEL and live provider credentials"]
fn credential_gated_provider_path() {
    let model = std::env::var("RESEARCH_PIPELINE_CHAT_MODEL")
        .expect("set RESEARCH_PIPELINE_CHAT_MODEL before running the ignored live smoke");
    let materials = fs::canonicalize(
        repository_root().join("examples/demo/research-pipeline/fixtures/research"),
    )
    .expect("resolve the shared Research Pipeline fixture corpus");

    let output = Command::new(env!("CARGO_BIN_EXE_research-pipeline"))
        .arg("run")
        .arg("--question")
        .arg("Summarize the strongest evidence in the fixture corpus.")
        .arg("--materials")
        .arg(materials)
        .env("RESEARCH_PIPELINE_CHAT_MODEL", model)
        .output()
        .expect("run the credential-gated Research Pipeline provider smoke");

    assert!(
        output.status.success(),
        "live provider smoke failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}
