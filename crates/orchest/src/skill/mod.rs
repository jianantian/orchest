//! Skill system: scanning, types, environment management, and bundled tools.

pub mod bundled_tool;
pub mod disclosure;
pub mod env_manager;
pub mod executor;
pub mod scanner;
pub mod types;

pub use env_manager::{CapabilityValidator, SkillEnvManager};
pub use scanner::SkillScanner;
pub use types::{
    BundledToolDef, EnvError, ScanOutcome, ScanWarning, SkillCapabilities, SkillDependencies,
    SkillManifest,
};
