//! Configuration management

use anyhow::Result;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config {
    #[serde(default = "default_port")]
    pub port: u16,

    #[serde(default)]
    pub roon: RoonConfig,

    #[serde(default)]
    pub ai: Option<AiConfig>,
}

fn default_port() -> u16 {
    8088
}

#[derive(Debug, Default, Deserialize)]
pub struct AiConfig {
    /// Anthropic API key — also read from ANTHROPIC_API_KEY env var
    pub api_key: Option<String>,
}

/// Resolve the Anthropic API key: env var takes precedence over TOML
pub fn resolve_anthropic_api_key(config: &Config) -> Option<String> {
    std::env::var("ANTHROPIC_API_KEY")
        .ok()
        .or_else(|| config.ai.as_ref().and_then(|a| a.api_key.clone()))
}

#[derive(Debug, Default, Deserialize)]
pub struct RoonConfig {
    pub extension_id: Option<String>,
    pub display_name: Option<String>,
}

/// Subdirectory name for state config files
/// Issue #76: Organize config files into a subdirectory to avoid clutter
const CONFIG_SUBDIR_NAME: &str = "state";

/// Config files that should be migrated to the subdirectory
const MIGRATABLE_CONFIG_FILES: &[&str] = &[
    "app-settings.json",
    "roon_state.json",
];

/// Get config directory (XDG_CONFIG_HOME or platform default)
pub fn get_config_dir() -> std::path::PathBuf {
    // Check ROON_AI-specific env var first
    if let Ok(dir) = std::env::var("ROON_AI_CONFIG_DIR") {
        return std::path::PathBuf::from(dir);
    }
    // Support Node.js CONFIG_DIR for seamless migration
    if let Ok(dir) = std::env::var("CONFIG_DIR") {
        return std::path::PathBuf::from(dir);
    }

    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = std::env::var("HOME") {
            return std::path::PathBuf::from(home)
                .join("Library/Application Support/roon-ai");
        }
    }

    #[cfg(target_os = "linux")]
    {
        if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
            return std::path::PathBuf::from(xdg).join("roon-ai");
        }
        if let Ok(home) = std::env::var("HOME") {
            return std::path::PathBuf::from(home).join(".config/roon-ai");
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            return std::path::PathBuf::from(appdata).join("roon-ai");
        }
    }

    // Fallback to current directory
    std::path::PathBuf::from(".")
}

/// Get config subdirectory for state config files
/// Issue #76: Organize config files into state/ subdirectory
pub fn get_config_subdir() -> std::path::PathBuf {
    get_config_dir().join(CONFIG_SUBDIR_NAME)
}

/// Get the path for a config file (always in subdirectory for new writes)
/// Issue #76: New files are written to the subdirectory
pub fn get_config_file_path(filename: &str) -> std::path::PathBuf {
    get_config_subdir().join(filename)
}

/// Read a config file with backwards compatibility fallback
/// Issue #76: Check subdirectory first, fall back to root for legacy files
pub fn read_config_file(filename: &str) -> Option<String> {
    let subdir_path = get_config_subdir().join(filename);
    let root_path = get_config_dir().join(filename);

    // Try subdirectory first (new location)
    if subdir_path.exists() {
        return std::fs::read_to_string(&subdir_path).ok();
    }

    // Fall back to root (legacy location)
    if root_path.exists() {
        return std::fs::read_to_string(&root_path).ok();
    }

    None
}

/// Migrate config files from root directory to subdirectory
/// Issue #76: On startup, move config files to state/ subdirectory
pub fn migrate_config_to_subdir() {
    let config_dir = get_config_dir();
    let data_dir = get_data_dir();
    let subdir = config_dir.join(CONFIG_SUBDIR_NAME);

    // Ensure subdirectory exists
    if let Err(e) = std::fs::create_dir_all(&subdir) {
        tracing::warn!("Failed to create config subdirectory: {}", e);
        return;
    }

    // Migrate each config file from config dir root
    for filename in MIGRATABLE_CONFIG_FILES {
        migrate_single_file(&config_dir, &subdir, filename);
    }

    // Also check data directory for roon_state.json (may differ from config dir on Linux)
    // This handles the case where roon_state.json was previously in XDG_DATA_HOME
    if data_dir != config_dir {
        migrate_single_file(&data_dir, &subdir, "roon_state.json");
    }
}

/// Migrate a single file from source directory to subdirectory
fn migrate_single_file(source_dir: &std::path::Path, subdir: &std::path::Path, filename: &str) {
    let source_path = source_dir.join(filename);
    let subdir_path = subdir.join(filename);

    // Skip if file doesn't exist at source
    if !source_path.exists() {
        return;
    }

    // Don't overwrite existing files in subdirectory
    if subdir_path.exists() {
        tracing::debug!(
            "Skipping migration of {} (already exists in subdirectory)",
            filename
        );
        return;
    }

    // Move file from source to subdirectory
    match std::fs::rename(&source_path, &subdir_path) {
        Ok(()) => {
            tracing::info!(
                "Migrated config file: {} -> state/{}",
                filename,
                filename
            );
        }
        Err(e) => {
            // If rename fails (e.g., cross-device), try copy + delete
            match std::fs::read(&source_path) {
                Ok(content) => {
                    if let Err(e) = std::fs::write(&subdir_path, &content) {
                        tracing::warn!("Failed to write migrated config {}: {}", filename, e);
                        return;
                    }
                    if let Err(e) = std::fs::remove_file(&source_path) {
                        tracing::warn!(
                            "Migrated {} but failed to remove original: {}",
                            filename,
                            e
                        );
                    } else {
                        tracing::info!(
                            "Migrated config file (copy): {} -> state/{}",
                            filename,
                            filename
                        );
                    }
                }
                Err(_) => {
                    tracing::warn!("Failed to migrate config {}: {}", filename, e);
                }
            }
        }
    }
}

/// Get data directory (XDG_DATA_HOME or platform default)
pub fn get_data_dir() -> std::path::PathBuf {
    // Check ROON_AI-specific env var first
    if let Ok(dir) = std::env::var("ROON_AI_DATA_DIR") {
        return std::path::PathBuf::from(dir);
    }
    // Support Node.js CONFIG_DIR for seamless migration (Node.js uses same dir for config and data)
    if let Ok(dir) = std::env::var("CONFIG_DIR") {
        return std::path::PathBuf::from(dir);
    }

    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = std::env::var("HOME") {
            return std::path::PathBuf::from(home)
                .join("Library/Application Support/roon-ai");
        }
    }

    #[cfg(target_os = "linux")]
    {
        if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
            return std::path::PathBuf::from(xdg).join("roon-ai");
        }
        if let Ok(home) = std::env::var("HOME") {
            return std::path::PathBuf::from(home).join(".local/share/roon-ai");
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(appdata) = std::env::var("LOCALAPPDATA") {
            return std::path::PathBuf::from(appdata).join("roon-ai");
        }
    }

    // Fallback to ./data
    std::path::PathBuf::from("./data")
}

pub fn load_config() -> Result<Config> {
    let config_dir = get_config_dir();

    let mut builder = ::config::Config::builder()
        // Start with defaults
        .set_default("port", 8088)?
        // Load from config file if it exists
        .add_source(
            ::config::File::with_name(&config_dir.join("config").to_string_lossy()).required(false),
        )
        // Override with environment variables (ROON_AI_PORT, ROON_AI_ROON__EXTENSION_ID, etc.)
        .add_source(
            ::config::Environment::with_prefix("ROON_AI")
                .separator("__")
                .try_parsing(true),
        );

    // Support PORT env vars with explicit precedence: ROON_AI_PORT > PORT > config > default
    // Handle manually to ensure consistent behavior across all environments
    if let Ok(port) = std::env::var("ROON_AI_PORT") {
        if let Ok(port_num) = port.parse::<u16>() {
            builder = builder.set_override("port", port_num as i64)?;
        }
    } else if let Ok(port) = std::env::var("PORT") {
        // Legacy PORT fallback (used by LMS plugin Helper.pm, Docker, etc.)
        if let Ok(port_num) = port.parse::<u16>() {
            builder = builder.set_override("port", port_num as i64)?;
        }
    }

    let config = builder.build()?;

    Ok(config.try_deserialize()?)
}

/// Migrate Node.js config files to Rust format on startup
///
/// This function runs once at startup to seamlessly import Node.js configs:
/// - roon-config.json → roon_state.json (Roon pairing state)
/// - app-settings.json (handled by serde aliases in AppSettings)
pub fn migrate_nodejs_configs() {
    let data_dir = get_data_dir();

    // Ensure data directory exists
    if let Err(e) = std::fs::create_dir_all(&data_dir) {
        tracing::warn!("Failed to create data directory: {}", e);
        return;
    }

    // Migrate Roon config (roon-config.json → roon_state.json)
    migrate_roon_config(&data_dir);

    tracing::debug!("Node.js config migration check complete");
}

/// Migrate Roon config from Node.js format
fn migrate_roon_config(data_dir: &std::path::Path) {
    let nodejs_path = data_dir.join("roon-config.json");
    let rust_path = data_dir.join("roon_state.json");

    // Only migrate if Node.js config exists and Rust config doesn't
    if nodejs_path.exists() && !rust_path.exists() {
        match std::fs::read_to_string(&nodejs_path) {
            Ok(content) => {
                // The format is compatible - both use the same Roon API state structure
                match std::fs::write(&rust_path, &content) {
                    Ok(()) => {
                        tracing::info!(
                            "Migrated Roon config from Node.js: {} → {}",
                            nodejs_path.display(),
                            rust_path.display()
                        );
                    }
                    Err(e) => tracing::warn!("Failed to write Roon state file: {}", e),
                }
            }
            Err(e) => tracing::warn!("Failed to read Node.js Roon config: {}", e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::env;

    /// RAII guard for env vars - restores original value (or removes) on drop
    struct EnvGuard {
        key: &'static str,
        original: Option<String>,
    }

    impl EnvGuard {
        fn set(key: &'static str, value: impl AsRef<str>) -> Self {
            let original = env::var(key).ok();
            env::set_var(key, value.as_ref());
            Self { key, original }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match &self.original {
                Some(v) => env::set_var(self.key, v),
                None => env::remove_var(self.key),
            }
        }
    }

    #[test]
    #[serial]
    fn test_port_env_fallback() {
        // Issue #75: PORT env var should work as fallback when ROON_AI_PORT is not set
        let _g1 = EnvGuard::set("ROON_AI_CONFIG_DIR", "/tmp/roon-ai-test-nonexistent");
        let _g2 = EnvGuard::set("PORT", "3000");
        env::remove_var("ROON_AI_PORT"); // Ensure ROON_AI_PORT not set

        let config = load_config().expect("config should load");

        assert_eq!(config.port, 3000, "PORT env var should set config.port");
    }

    #[test]
    #[serial]
    fn test_roon_ai_port_takes_precedence_over_port() {
        // Issue #75: ROON_AI_PORT should take precedence over legacy PORT
        let _g1 = EnvGuard::set("ROON_AI_CONFIG_DIR", "/tmp/roon-ai-test-nonexistent");
        let _g2 = EnvGuard::set("ROON_AI_PORT", "5000");
        let _g3 = EnvGuard::set("PORT", "3000");

        let config = load_config().expect("config should load");

        assert_eq!(
            config.port, 5000,
            "ROON_AI_PORT should take precedence over PORT"
        );
    }

    #[test]
    #[serial]
    fn test_invalid_port_uses_default() {
        // Invalid PORT value should fall back to default (8088)
        let _g1 = EnvGuard::set("ROON_AI_CONFIG_DIR", "/tmp/roon-ai-test-nonexistent");
        let _g2 = EnvGuard::set("PORT", "not-a-number");
        env::remove_var("ROON_AI_PORT"); // Ensure ROON_AI_PORT not set

        let config = load_config().expect("config should load");

        assert_eq!(
            config.port, 8088,
            "Invalid PORT should fall back to default"
        );
    }

    // =========================================================================
    // Issue #76: Config subdirectory organization tests
    // =========================================================================

    #[test]
    #[serial]
    fn test_get_config_subdir_returns_unified_hifi_subdir() {
        // Issue #76: get_config_subdir() should return state/ subdirectory
        let temp_dir = tempfile::tempdir().expect("create temp dir");
        env::set_var("ROON_AI_CONFIG_DIR", temp_dir.path());

        let subdir = get_config_subdir();

        env::remove_var("ROON_AI_CONFIG_DIR");

        assert!(
            subdir.ends_with("state"),
            "subdir should end with 'state', got: {:?}",
            subdir
        );
        assert_eq!(
            subdir.parent().unwrap(),
            temp_dir.path(),
            "parent should be config dir"
        );
    }

    #[test]
    #[serial]
    fn test_migrate_config_files_to_subdir() {
        // Issue #76: migrate_config_to_subdir() should move files from root to subdirectory
        let temp_dir = tempfile::tempdir().expect("create temp dir");
        let config_dir = temp_dir.path();

        // Create test config files at root level
        let files = ["app-settings.json"];
        for file in &files {
            std::fs::write(config_dir.join(file), r#"{"test": true}"#).expect("write file");
        }

        env::set_var("ROON_AI_CONFIG_DIR", config_dir);

        // Run migration
        migrate_config_to_subdir();

        env::remove_var("ROON_AI_CONFIG_DIR");

        // Verify files moved to subdirectory
        let subdir = config_dir.join("state");
        assert!(subdir.exists(), "subdirectory should be created");

        for file in &files {
            assert!(
                subdir.join(file).exists(),
                "file {} should exist in subdirectory",
                file
            );
            assert!(
                !config_dir.join(file).exists(),
                "file {} should not exist at root",
                file
            );
        }
    }

    #[test]
    #[serial]
    fn test_migration_skips_if_subdir_exists() {
        // Issue #76: If subdir already has files, don't overwrite them
        let temp_dir = tempfile::tempdir().expect("create temp dir");
        let config_dir = temp_dir.path();
        let subdir = config_dir.join("state");

        // Create subdirectory with existing config
        std::fs::create_dir_all(&subdir).expect("create subdir");
        std::fs::write(subdir.join("app-settings.json"), r#"{"existing": true}"#)
            .expect("write existing");

        // Create file at root (should not overwrite subdir file)
        std::fs::write(config_dir.join("app-settings.json"), r#"{"root": true}"#)
            .expect("write root");

        env::set_var("ROON_AI_CONFIG_DIR", config_dir);

        migrate_config_to_subdir();

        env::remove_var("ROON_AI_CONFIG_DIR");

        // Verify existing subdir file was not overwritten
        let content =
            std::fs::read_to_string(subdir.join("app-settings.json")).expect("read subdir file");
        assert!(
            content.contains("existing"),
            "subdir file should not be overwritten"
        );
    }

    #[test]
    #[serial]
    fn test_get_config_file_path_prefers_subdir() {
        // Issue #76: get_config_file_path() should check subdir first, fall back to root
        let temp_dir = tempfile::tempdir().expect("create temp dir");
        let config_dir = temp_dir.path();
        let subdir = config_dir.join("state");

        // Create file only in subdir
        std::fs::create_dir_all(&subdir).expect("create subdir");
        std::fs::write(subdir.join("lms-config.json"), r#"{"subdir": true}"#)
            .expect("write subdir");

        env::set_var("ROON_AI_CONFIG_DIR", config_dir);

        let path = get_config_file_path("lms-config.json");

        env::remove_var("ROON_AI_CONFIG_DIR");

        assert_eq!(path, subdir.join("lms-config.json"));
    }

    #[test]
    #[serial]
    fn test_get_config_file_path_always_returns_subdir() {
        // Issue #76: get_config_file_path() always returns subdir path for writes
        // Note: read_config_file() handles fallback to root for legacy files
        let temp_dir = tempfile::tempdir().expect("create temp dir");
        let config_dir = temp_dir.path();

        // Create file only at root (legacy location)
        std::fs::write(config_dir.join("lms-config.json"), r#"{"root": true}"#)
            .expect("write root");

        env::set_var("ROON_AI_CONFIG_DIR", config_dir);

        let path = get_config_file_path("lms-config.json");

        env::remove_var("ROON_AI_CONFIG_DIR");

        // Should return subdir path (for new writes), even though file exists at root
        // The file reading logic handles fallback
        assert!(
            path.to_string_lossy().contains("state"),
            "path should be in state subdir for new writes"
        );
    }

    #[test]
    #[serial]
    fn test_read_config_file_with_fallback() {
        // Issue #76: Reading config should check subdir first, fall back to root
        let temp_dir = tempfile::tempdir().expect("create temp dir");
        let config_dir = temp_dir.path();

        // Create file only at root (legacy location)
        std::fs::write(config_dir.join("lms-config.json"), r#"{"legacy": true}"#)
            .expect("write root");

        env::set_var("ROON_AI_CONFIG_DIR", config_dir);

        let content = read_config_file("lms-config.json");

        env::remove_var("ROON_AI_CONFIG_DIR");

        assert!(content.is_some(), "should find legacy file at root");
        assert!(
            content.unwrap().contains("legacy"),
            "should read legacy content"
        );
    }
}

