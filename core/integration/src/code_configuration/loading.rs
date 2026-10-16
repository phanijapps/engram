use super::{
    environment,
    resolve::{ResolvedCodeConfig, invalid, resolve_code_config},
};
use engram_retrieval::CodeIntelligenceConfigOverlay;
use engram_runtime::CoreResult;
use std::io::Read;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

/// Inputs captured by a trusted host once; scanned repositories are not consulted.
#[derive(Debug, Clone, Default)]
pub struct CodeConfigLaunch {
    pub file: Option<PathBuf>,
    pub file_overlay: Option<CodeIntelligenceConfigOverlay>,
    pub environment: BTreeMap<String, String>,
    pub explicit: Option<CodeIntelligenceConfigOverlay>,
}

fn read(path: &Path) -> CoreResult<CodeIntelligenceConfigOverlay> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|e| invalid(format!("read code config {}: {e}", path.display())))?
        .take(65537)
        .read_to_end(&mut bytes)
        .map_err(|e| invalid(format!("read code config {}: {e}", path.display())))?;
    if bytes.len() > 65536 {
        return Err(invalid("code config exceeds 64 KiB"));
    }
    let text = std::str::from_utf8(&bytes).map_err(|_| invalid("code config must be UTF-8"))?;
    match path.extension().and_then(|v| v.to_str()) {
        Some("json") => serde_json::from_str(text).map_err(|e| {
            invalid(format!(
                "code config JSON from file {}: {e}",
                path.display()
            ))
        }),
        Some("toml") => toml::from_str(text).map_err(|e| {
            invalid(format!(
                "code config TOML from file {}: {e}",
                path.display()
            ))
        }),
        _ => Err(invalid("code config must have a .json or .toml extension")),
    }
}

pub fn load_code_config(launch: &CodeConfigLaunch) -> CoreResult<ResolvedCodeConfig> {
    let env = environment::parse(&launch.environment)?;
    let file = launch.file.clone().or_else(|| {
        launch
            .environment
            .get("ENGRAM_CODE_CONFIG")
            .map(PathBuf::from)
    });
    let overlay = file
        .as_deref()
        .map(read)
        .transpose()?
        .or_else(|| launch.file_overlay.clone());
    resolve_code_config(overlay.as_ref(), Some(&env), launch.explicit.as_ref())
}
