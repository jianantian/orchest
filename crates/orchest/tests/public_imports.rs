//! Compile tests for the #256 public-import ergonomics.
//!
//! Proves both the canonical shallow paths and the preserved deep module
//! paths resolve to the same types.

#[test]
fn llm_watcher_shallow_and_deep_paths_are_the_same_type() {
    fn assert_same_type<T>(_a: *const T, _b: *const T) {}

    assert_same_type(
        std::ptr::null::<orchest::run::LlmWatcher>(),
        std::ptr::null::<orchest::run::llm_watcher::LlmWatcher>(),
    );
    assert_same_type(
        std::ptr::null::<orchest::LlmWatcher>(),
        std::ptr::null::<orchest::run::llm_watcher::LlmWatcher>(),
    );
}

#[test]
fn context_mode_shallow_and_deep_paths_are_the_same_type() {
    fn assert_same_type<T>(_a: *const T, _b: *const T) {}

    assert_same_type(
        std::ptr::null::<orchest::tool::ContextMode>(),
        std::ptr::null::<orchest::tool::agent_as_tool::ContextMode>(),
    );

    // Exercise the shallow path as a value, not only a type name.
    let mode = orchest::tool::ContextMode::Fresh;
    assert_eq!(mode, orchest::tool::agent_as_tool::ContextMode::Fresh);
}
