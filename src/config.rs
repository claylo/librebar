//! Configuration discovery, loading, and merging.
//!
//! Provides format-agnostic config file discovery, layered merging, and
//! deserialization into user-defined config types.
//!
//! # Supported formats
//!
//! - TOML (`.toml`)
//! - YAML (`.yaml`, `.yml`)
//! - JSON (`.json`)
//!
//! # Merge order (lowest to highest precedence)
//!
//! 1. `C::default()` — struct defaults from `#[serde(default)]`
//! 2. User config — `~/.config/{app}/config.{ext}` (XDG on macOS/Linux)
//! 3. Project config — found by walking up from cwd (`.config/{app}.ext`,
//!    `.{app}.ext`, `{app}.ext`)
//! 4. Explicit files — passed via [`ConfigLoader::with_file()`]
//!
//! All layers are parsed into `serde_json::Value` and deep-merged
//! (objects merge recursively, scalars/arrays replace). The merged
//! result is deserialized into the user's config type.
//!
//! # Discovery
//!
//! Project config search walks up from the search root, checking each
//! directory for config files in this order per directory:
//! 1. `.config/{app}.{ext}` (dotconfig directory)
//! 2. `.{app}.{ext}` (dotfile)
//! 3. `{app}.{ext}` (plain file)
//!
//! Search stops at a `.git` boundary by default (configurable via
//! [`ConfigLoader::with_boundary_marker()`]).

use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{Error, Result};

/// Supported configuration file extensions (in order of preference).
const CONFIG_EXTENSIONS: &[&str] = &["toml", "yaml", "yml", "json"];

// ─── LogLevel ───────────────────────────────────────────────────────

/// Log level configuration, deserializable from config files.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    /// Verbose output for debugging and development.
    Debug,
    /// Standard operational information (default).
    #[default]
    Info,
    /// Warnings about potential issues.
    Warn,
    /// Errors that indicate failures.
    Error,
}

impl LogLevel {
    /// Returns the log level as a lowercase string slice.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }
}

// ─── ConfigSources ──────────────────────────────────────────────────

/// Metadata about which configuration sources were loaded.
///
/// Returned alongside the config from [`ConfigLoader::load()`] so commands
/// like `doctor` and `info` can report the actual config files.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ConfigSources {
    /// Project config file found by walking up from the search root.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_file: Option<Utf8PathBuf>,
    /// User config file from XDG config directory.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_file: Option<Utf8PathBuf>,
    /// Explicit config files loaded (e.g., from `--config` flag).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub explicit_files: Vec<Utf8PathBuf>,
}

impl ConfigSources {
    /// Returns the highest-precedence config file that was loaded.
    ///
    /// Precedence: explicit files > project file > user file.
    pub fn primary_file(&self) -> Option<&Utf8Path> {
        self.explicit_files
            .last()
            .map(Utf8PathBuf::as_path)
            .or(self.project_file.as_deref())
            .or(self.user_file.as_deref())
    }
}

// ─── Deep Merge ─────────────────────────────────────────────────────

/// Deep-merge `overlay` into `base`.
///
/// - Objects: recursively merge, overlay keys win.
/// - Scalars and arrays: overlay replaces base.
pub fn deep_merge(base: &mut Value, overlay: Value) {
    match (base, overlay) {
        (Value::Object(base_map), Value::Object(overlay_map)) => {
            for (key, value) in overlay_map {
                deep_merge(base_map.entry(key).or_insert(Value::Null), value);
            }
        }
        (base, overlay) => *base = overlay,
    }
}

// ─── File Parsing ───────────────────────────────────────────────────

/// Parse TOML content into a `serde_json::Value`.
///
/// # Errors
///
/// Returns [`Error::ConfigParse`] if the content is not valid TOML.
pub fn parse_toml(content: &str) -> Result<Value> {
    let toml_value: toml::Value = toml::from_str(content).map_err(|e| Error::ConfigParse {
        path: "<toml>".to_string(),
        source: Box::new(e),
    })?;
    serde_json::to_value(toml_value).map_err(|e| Error::ConfigDeserialize(Box::new(e)))
}

/// Parse YAML content into a `serde_json::Value`.
///
/// # Errors
///
/// Returns [`Error::ConfigParse`] if the content is not valid YAML.
pub fn parse_yaml(content: &str) -> Result<Value> {
    serde_saphyr::from_str(content).map_err(|e| Error::ConfigParse {
        path: "<yaml>".to_string(),
        source: Box::new(e),
    })
}

/// Parse JSON content into a `serde_json::Value`.
///
/// # Errors
///
/// Returns [`Error::ConfigParse`] if the content is not valid JSON.
pub fn parse_json(content: &str) -> Result<Value> {
    serde_json::from_str(content).map_err(|e| Error::ConfigParse {
        path: "<json>".to_string(),
        source: Box::new(e),
    })
}

/// Parse a config file, detecting format from extension.
///
/// # Errors
///
/// Returns an error if the file cannot be read or parsed.
pub fn parse_file(path: &Utf8Path) -> Result<Value> {
    let content = std::fs::read_to_string(path.as_str()).map_err(|e| Error::ConfigParse {
        path: path.to_string(),
        source: Box::new(e),
    })?;

    match path.extension() {
        Some("toml") => parse_toml(&content),
        Some("yaml" | "yml") => parse_yaml(&content),
        Some("json") => parse_json(&content),
        _ => parse_toml(&content), // default to TOML
    }
    // Replace the placeholder path ("<toml>", etc.) from the format-specific
    // parsers with the actual file path for better error messages.
    .map_err(|e| match e {
        Error::ConfigParse { source, .. } => Error::ConfigParse {
            path: path.to_string(),
            source,
        },
        other => other,
    })
}

// ─── ConfigLoader ───────────────────────────────────────────────────

/// Builder for loading configuration from multiple sources.
///
/// Discovers config files by walking up directories, loads user config
/// from XDG directories, merges all sources, and deserializes into the
/// consumer's config type.
#[derive(Debug, Default)]
pub struct ConfigLoader {
    app_name: String,
    project_search_root: Option<Utf8PathBuf>,
    include_user_config: bool,
    boundary_marker: Option<String>,
    explicit_files: Vec<Utf8PathBuf>,
}

impl ConfigLoader {
    /// Create a new config loader for the given application name.
    ///
    /// The app name is used for XDG directory lookup and config file names.
    pub fn new(app_name: &str) -> Self {
        Self {
            app_name: app_name.to_string(),
            project_search_root: None,
            include_user_config: true,
            boundary_marker: Some(".git".to_string()),
            explicit_files: Vec::new(),
        }
    }

    /// Set the starting directory for project config search.
    pub fn with_project_search<P: AsRef<Utf8Path>>(mut self, path: P) -> Self {
        self.project_search_root = Some(path.as_ref().to_path_buf());
        self
    }

    /// Set whether to include user config from XDG directory.
    pub const fn with_user_config(mut self, include: bool) -> Self {
        self.include_user_config = include;
        self
    }

    /// Set a boundary marker to stop directory traversal (default: `.git`).
    pub fn with_boundary_marker<S: Into<String>>(mut self, marker: S) -> Self {
        self.boundary_marker = Some(marker.into());
        self
    }

    /// Disable boundary marker (search all the way to filesystem root).
    pub fn without_boundary_marker(mut self) -> Self {
        self.boundary_marker = None;
        self
    }

    /// Add an explicit config file to load (highest precedence).
    pub fn with_file<P: AsRef<Utf8Path>>(mut self, path: P) -> Self {
        self.explicit_files.push(path.as_ref().to_path_buf());
        self
    }

    /// Load configuration, merging all discovered sources.
    ///
    /// Returns the merged config alongside metadata about which files
    /// were loaded.
    ///
    /// # Errors
    ///
    /// Returns an error if an explicit file cannot be read or parsed,
    /// or if the merged result cannot be deserialized into `C`.
    #[tracing::instrument(skip(self), fields(app = %self.app_name, search_root = ?self.project_search_root))]
    pub fn load<C: serde::de::DeserializeOwned + Default + Serialize>(
        self,
    ) -> Result<(C, ConfigSources)> {
        tracing::debug!("loading configuration");
        let mut merged = serde_json::to_value(C::default())
            .map_err(|e| Error::ConfigDeserialize(Box::new(e)))?;
        let mut sources = ConfigSources::default();

        // User config (lowest precedence of file sources)
        if self.include_user_config
            && let Some(user_config) = self.find_user_config()
            && let Ok(value) = parse_file(&user_config)
        {
            deep_merge(&mut merged, value);
            sources.user_file = Some(user_config);
        }

        // Project config
        if let Some(ref root) = self.project_search_root
            && let Some(project_config) = self.find_project_config(root)
            && let Ok(value) = parse_file(&project_config)
        {
            deep_merge(&mut merged, value);
            sources.project_file = Some(project_config);
        }

        // Explicit files (highest precedence)
        for file in &self.explicit_files {
            let value = parse_file(file)?;
            deep_merge(&mut merged, value);
        }
        sources.explicit_files = self.explicit_files;

        let config: C =
            serde_json::from_value(merged).map_err(|e| Error::ConfigDeserialize(Box::new(e)))?;
        tracing::info!("configuration loaded");
        Ok((config, sources))
    }

    /// Load configuration, returning an error if no config file is found.
    ///
    /// # Errors
    ///
    /// Returns [`Error::ConfigNotFound`] if no config files exist.
    pub fn load_or_error<C: serde::de::DeserializeOwned + Default + Serialize>(
        &self,
    ) -> Result<(C, ConfigSources)> {
        let has_user = self.include_user_config && self.find_user_config().is_some();
        let has_project = self
            .project_search_root
            .as_ref()
            .and_then(|root| self.find_project_config(root))
            .is_some();
        let has_explicit = !self.explicit_files.is_empty();

        if !has_user && !has_project && !has_explicit {
            return Err(Error::ConfigNotFound);
        }

        // Clone self's fields to create a new loader for the actual load
        Self {
            app_name: self.app_name.clone(),
            project_search_root: self.project_search_root.clone(),
            include_user_config: self.include_user_config,
            boundary_marker: self.boundary_marker.clone(),
            explicit_files: self.explicit_files.clone(),
        }
        .load()
    }

    /// Find project config by walking up from the given directory.
    fn find_project_config(&self, start: &Utf8Path) -> Option<Utf8PathBuf> {
        let mut current = Some(start.to_path_buf());

        while let Some(dir) = current {
            for ext in CONFIG_EXTENSIONS {
                // .config/app.ext
                let dotconfig = dir.join(format!(".config/{}.{ext}", self.app_name));
                if dotconfig.is_file() {
                    return Some(dotconfig);
                }

                // .app.ext
                let dotfile = dir.join(format!(".{}.{ext}", self.app_name));
                if dotfile.is_file() {
                    return Some(dotfile);
                }

                // app.ext
                let regular = dir.join(format!("{}.{ext}", self.app_name));
                if regular.is_file() {
                    return Some(regular);
                }
            }

            // Check boundary after checking config (so same-dir config is found)
            if let Some(ref marker) = self.boundary_marker
                && dir.join(marker).exists()
                && dir != start
            {
                break;
            }

            current = dir.parent().map(Utf8Path::to_path_buf);
        }

        None
    }

    /// Find user config in XDG config directory.
    fn find_user_config(&self) -> Option<Utf8PathBuf> {
        let proj_dirs = directories::ProjectDirs::from("", "", &self.app_name)?;
        let config_dir = proj_dirs.config_dir();

        for ext in CONFIG_EXTENSIONS {
            let config_path = config_dir.join(format!("config.{ext}"));
            if config_path.is_file() {
                return Utf8PathBuf::from_path_buf(config_path).ok();
            }
        }

        None
    }
}

// ─── XDG Helpers ────────────────────────────────────────────────────

/// Get the user config directory for an application.
pub fn user_config_dir(app_name: &str) -> Option<Utf8PathBuf> {
    let proj_dirs = directories::ProjectDirs::from("", "", app_name)?;
    Utf8PathBuf::from_path_buf(proj_dirs.config_dir().to_path_buf()).ok()
}

/// Get the user cache directory for an application.
pub fn user_cache_dir(app_name: &str) -> Option<Utf8PathBuf> {
    let proj_dirs = directories::ProjectDirs::from("", "", app_name)?;
    Utf8PathBuf::from_path_buf(proj_dirs.cache_dir().to_path_buf()).ok()
}

/// Get the user data directory for an application.
pub fn user_data_dir(app_name: &str) -> Option<Utf8PathBuf> {
    let proj_dirs = directories::ProjectDirs::from("", "", app_name)?;
    Utf8PathBuf::from_path_buf(proj_dirs.data_dir().to_path_buf()).ok()
}
