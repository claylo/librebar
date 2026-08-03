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
//! 4. Environment variables — `{APP}_{FIELD}`, with `__` for nesting
//! 5. Explicit files — passed via [`ConfigLoader::with_file()`]
//! 6. Programmatic overrides — passed via [`ConfigLoader::with_override()`]
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

use std::collections::{BTreeMap, HashSet};
use std::ffi::{OsStr, OsString};

use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{ConfigParseError, Error, Result, boxed_error};

/// Re-export of [`serde_json`], used by the dynamic config APIs.
pub use serde_json;

mod environment;
pub use environment::{EnvironmentSource, ProcessEnvironment, UnknownEnvironment};

/// Supported configuration file extensions (in order of preference).
const CONFIG_EXTENSIONS: &[&str] = &["toml", "yaml", "yml", "json"];

// ─── LogLevel ───────────────────────────────────────────────────────

/// Log level configuration, deserializable from config files.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    /// Maximum diagnostic detail.
    Trace,
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
            Self::Trace => "trace",
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }
}

// ─── ConfigSources ──────────────────────────────────────────────────

/// The winning source for a merged configuration value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
#[non_exhaustive]
pub enum ConfigOrigin {
    /// Value came from the consumer config type's default.
    Default,
    /// Value came from a preloaded consumer configuration.
    Preloaded,
    /// Value came from the user config file.
    UserFile {
        /// Loaded file path.
        path: Utf8PathBuf,
    },
    /// Value came from the discovered project config file.
    ProjectFile {
        /// Loaded file path.
        path: Utf8PathBuf,
    },
    /// Value came from an environment variable.
    Environment {
        /// Environment variable name. Its value is never retained.
        variable: String,
    },
    /// Value came from an explicitly selected config file.
    ExplicitFile {
        /// Loaded file path.
        path: Utf8PathBuf,
    },
    /// Value came from a programmatic override.
    Override {
        /// Dotted override path.
        path: String,
    },
}

impl std::fmt::Display for ConfigOrigin {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Default => formatter.write_str("defaults"),
            Self::Preloaded => formatter.write_str("preloaded configuration"),
            Self::UserFile { path } => write!(formatter, "user config file {path}"),
            Self::ProjectFile { path } => write!(formatter, "project config file {path}"),
            Self::Environment { variable } => {
                write!(formatter, "environment variable {variable}")
            }
            Self::ExplicitFile { path } => write!(formatter, "explicit config file {path}"),
            Self::Override { path } => write!(formatter, "programmatic override {path}"),
        }
    }
}

pub(crate) type OriginMap = BTreeMap<String, ConfigOrigin>;

pub(super) fn record_origins(
    origins: &mut OriginMap,
    path: &str,
    value: &Value,
    origin: ConfigOrigin,
) {
    origins.insert(path.to_string(), origin.clone());
    match value {
        Value::Object(values) => {
            for (key, value) in values {
                let child = if path == "$" {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                record_origins(origins, &child, value, origin.clone());
            }
        }
        Value::Array(values) => {
            for (index, value) in values.iter().enumerate() {
                record_origins(origins, &format!("{path}[{index}]"), value, origin.clone());
            }
        }
        _ => {}
    }
}

/// Metadata about which configuration sources were loaded.
///
/// Returned alongside the config from [`ConfigLoader::load()`] so commands
/// like `doctor` and `info` can report loaded files and the winning source for
/// each merged value.
#[derive(Debug, Clone, Default, Serialize)]
#[non_exhaustive]
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
    /// Applied environment variable names. Values are never recorded.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub environment_variables: Vec<String>,
    /// Applied programmatic override paths.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub override_paths: Vec<String>,
    /// Winning source for each merged configuration path.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub value_origins: BTreeMap<String, ConfigOrigin>,
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

    /// Return the winning source for a merged configuration path.
    ///
    /// Paths use Serde's dotted field notation, such as `database.url`.
    pub fn origin(&self, path: &str) -> Option<&ConfigOrigin> {
        let mut candidate = if path.is_empty() || path == "." {
            "$"
        } else {
            path
        };
        loop {
            if let Some(origin) = self.value_origins.get(candidate) {
                return Some(origin);
            }
            let parent = parent_config_path(candidate)?;
            candidate = parent;
        }
    }

    pub(crate) fn record_layer(&mut self, value: &Value, origin: ConfigOrigin) {
        record_origins(&mut self.value_origins, "$", value, origin);
    }

    pub(crate) fn record_merge(&mut self, base: &Value, overlay: &Value, origin: ConfigOrigin) {
        let mut origins = OriginMap::new();
        record_origins(&mut origins, "$", overlay, origin);
        self.record_merge_origins(base, overlay, &origins);
    }

    pub(crate) fn record_merge_origins(
        &mut self,
        base: &Value,
        overlay: &Value,
        origins: &OriginMap,
    ) {
        merge_origins(&mut self.value_origins, Some(base), overlay, origins, "$");
    }

    const fn is_empty(&self) -> bool {
        self.project_file.is_none()
            && self.user_file.is_none()
            && self.explicit_files.is_empty()
            && self.environment_variables.is_empty()
            && self.override_paths.is_empty()
    }
}

fn merge_origins(
    current: &mut OriginMap,
    base: Option<&Value>,
    overlay: &Value,
    incoming: &OriginMap,
    path: &str,
) {
    if let (Some(Value::Object(base)), Value::Object(overlay)) = (base, overlay) {
        if let Some(origin) = incoming.get(path) {
            current.insert(path.to_string(), origin.clone());
        }
        for (key, value) in overlay {
            let child = if path == "$" {
                key.clone()
            } else {
                format!("{path}.{key}")
            };
            merge_origins(current, base.get(key), value, incoming, &child);
        }
        return;
    }

    let replacement_origin = incoming
        .get(path)
        .or_else(|| {
            incoming
                .iter()
                .find(|(candidate, _)| is_config_descendant(path, candidate))
                .map(|(_, origin)| origin)
        })
        .cloned();
    current.retain(|candidate, _| !is_config_descendant(path, candidate));
    current.extend(
        incoming
            .iter()
            .filter(|(candidate, _)| is_config_descendant(path, candidate))
            .map(|(path, origin)| (path.clone(), origin.clone())),
    );
    if !current.contains_key(path)
        && let Some(origin) = replacement_origin
    {
        current.insert(path.to_string(), origin);
    }
}

fn is_config_descendant(path: &str, candidate: &str) -> bool {
    path == "$"
        || candidate == path
        || candidate
            .strip_prefix(path)
            .is_some_and(|suffix| suffix.starts_with('.') || suffix.starts_with('['))
}

fn parent_config_path(path: &str) -> Option<&str> {
    if path == "$" {
        return None;
    }
    if path.ends_with(']')
        && let Some(index) = path.rfind('[')
    {
        return Some(if index == 0 { "$" } else { &path[..index] });
    }
    Some(path.rsplit_once('.').map_or("$", |(parent, _)| parent))
}

/// A serialized programmatic configuration override.
#[derive(Debug)]
pub(crate) struct ConfigOverride {
    path: String,
    value: std::result::Result<Value, serde_json::Error>,
}

impl ConfigOverride {
    pub(crate) fn new<V>(path: String, value: V) -> Self
    where
        V: Serialize,
    {
        Self {
            path,
            value: serde_json::to_value(value),
        }
    }
}

// ─── Deep Merge ─────────────────────────────────────────────────────

/// Maximum nesting depth for [`deep_merge`]. 64 levels is generous for
/// any config format — pathological inputs are rejected rather than
/// allowed to exhaust the stack.
const MERGE_DEPTH_LIMIT: usize = 64;

/// Deep-merge `overlay` into `base`.
///
/// - Objects: recursively merge, overlay keys win.
/// - Scalars and arrays: overlay replaces base.
///
/// # Errors
///
/// Returns [`Error::ConfigMergeDepth`] if the nesting depth exceeds 64 levels.
pub fn deep_merge(base: &mut Value, overlay: Value) -> Result<()> {
    fn merge_inner(base: &mut Value, overlay: Value, depth: usize) -> Result<()> {
        if depth > MERGE_DEPTH_LIMIT {
            return Err(crate::Error::ConfigMergeDepth);
        }
        match (base, overlay) {
            (Value::Object(base_map), Value::Object(overlay_map)) => {
                for (key, value) in overlay_map {
                    merge_inner(base_map.entry(key).or_insert(Value::Null), value, depth + 1)?;
                }
            }
            (base, overlay) => *base = overlay,
        }
        Ok(())
    }
    merge_inner(base, overlay, 0)
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
        source: Box::new(ConfigParseError::Toml(boxed_error(e))),
    })?;
    serde_json::to_value(toml_value).map_err(|error| Error::ConfigDeserialize(boxed_error(error)))
}

/// Parse YAML content into a `serde_json::Value`.
///
/// # Errors
///
/// Returns [`Error::ConfigParse`] if the content is not valid YAML.
pub fn parse_yaml(content: &str) -> Result<Value> {
    serde_saphyr::from_str(content).map_err(|e| Error::ConfigParse {
        path: "<yaml>".to_string(),
        source: Box::new(ConfigParseError::Yaml(boxed_error(e))),
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
        source: Box::new(ConfigParseError::Json(boxed_error(e))),
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
        source: Box::new(e.into()),
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
pub struct ConfigLoader {
    app_name: String,
    project_search_root: Option<Utf8PathBuf>,
    include_user_config: bool,
    boundary_marker: Option<String>,
    explicit_files: Vec<Utf8PathBuf>,
    environment_source: Option<std::sync::Arc<dyn EnvironmentSource>>,
    unknown_environment: UnknownEnvironment,
    overrides: Vec<ConfigOverride>,
}

impl std::fmt::Debug for ConfigLoader {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ConfigLoader")
            .field("app_name", &self.app_name)
            .field("project_search_root", &self.project_search_root)
            .field("include_user_config", &self.include_user_config)
            .field("boundary_marker", &self.boundary_marker)
            .field("explicit_files", &self.explicit_files)
            .field(
                "environment_source",
                &self
                    .environment_source
                    .as_ref()
                    .map(|_| "<environment source>"),
            )
            .field("unknown_environment", &self.unknown_environment)
            .field("overrides", &self.overrides)
            .finish()
    }
}

impl Default for ConfigLoader {
    fn default() -> Self {
        Self::new("")
    }
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
            environment_source: Some(std::sync::Arc::new(ProcessEnvironment)),
            unknown_environment: UnknownEnvironment::Ignore,
            overrides: Vec::new(),
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

    /// Add an explicit config file to load.
    ///
    /// Explicit files override discovered files and environment variables.
    /// Programmatic overrides still take final precedence.
    pub fn with_file<P: AsRef<Utf8Path>>(mut self, path: P) -> Self {
        self.explicit_files.push(path.as_ref().to_path_buf());
        self
    }

    /// Replace the process environment with a custom source.
    pub fn with_environment_source<E>(mut self, source: E) -> Self
    where
        E: EnvironmentSource + 'static,
    {
        self.environment_source = Some(std::sync::Arc::new(source));
        self
    }

    /// Disable environment configuration.
    pub fn without_environment(mut self) -> Self {
        self.environment_source = None;
        self
    }

    /// Configure handling for prefixed variables with unknown paths.
    pub const fn with_unknown_environment(mut self, policy: UnknownEnvironment) -> Self {
        self.unknown_environment = policy;
        self
    }

    /// Add a typed programmatic override at a dotted configuration path.
    pub fn with_override<V>(mut self, path: impl Into<String>, value: V) -> Self
    where
        V: Serialize,
    {
        self.overrides.push(ConfigOverride::new(path.into(), value));
        self
    }

    pub(crate) fn with_serialized_override(mut self, value: ConfigOverride) -> Self {
        self.overrides.push(value);
        self
    }

    /// Load configuration, merging all discovered sources.
    ///
    /// Returns the merged config alongside source and value-origin metadata.
    ///
    /// # Errors
    ///
    /// Returns an error if an explicit file cannot be read or parsed, an
    /// environment source cannot be queried, or the merged result cannot be
    /// deserialized into `C`.
    #[tracing::instrument(skip(self), fields(app = %self.app_name, search_root = ?self.project_search_root))]
    pub fn load<C: serde::de::DeserializeOwned + Default + Serialize>(
        self,
    ) -> Result<(C, ConfigSources)> {
        self.load_inner(false)
    }

    fn load_inner<C: serde::de::DeserializeOwned + Default + Serialize>(
        self,
        require_source: bool,
    ) -> Result<(C, ConfigSources)> {
        tracing::debug!("loading configuration");
        let mut merged = serde_json::to_value(C::default())
            .map_err(|error| Error::ConfigDeserialize(boxed_error(error)))?;
        let mut sources = ConfigSources::default();
        sources.record_layer(&merged, ConfigOrigin::Default);

        // User config (lowest precedence of file sources)
        if self.include_user_config
            && let Some(user_config) = self.find_user_config()
        {
            tracing::debug!(path = %user_config, "discovered user config");
            let value = parse_file(&user_config)?;
            sources.record_merge(
                &merged,
                &value,
                ConfigOrigin::UserFile {
                    path: user_config.clone(),
                },
            );
            deep_merge(&mut merged, value)?;
            sources.user_file = Some(user_config);
        }

        // Project config
        if let Some(ref root) = self.project_search_root
            && let Some(project_config) = self.find_project_config(root)
        {
            tracing::debug!(path = %project_config, "discovered project config");
            let value = parse_file(&project_config)?;
            sources.record_merge(
                &merged,
                &value,
                ConfigOrigin::ProjectFile {
                    path: project_config.clone(),
                },
            );
            deep_merge(&mut merged, value)?;
            sources.project_file = Some(project_config);
        }

        // Environment overrides discovered config, but not an explicitly
        // selected file.
        if let Some(source) = self.environment_source.as_deref() {
            let (overlay, variables, origins) =
                environment::overlay(&self.app_name, &merged, source, self.unknown_environment)?;
            sources.record_merge_origins(&merged, &overlay, &origins);
            deep_merge(&mut merged, overlay)?;
            sources.environment_variables = variables;
        }

        // Explicit files represent deliberate user selection and override
        // both discovered config and environment variables.
        for file in &self.explicit_files {
            tracing::debug!(path = %file, "loading explicit config");
            let value = parse_file(file)?;
            sources.record_merge(
                &merged,
                &value,
                ConfigOrigin::ExplicitFile { path: file.clone() },
            );
            deep_merge(&mut merged, value)?;
        }
        sources.explicit_files = self.explicit_files;

        sources.override_paths = apply_config_overrides(&mut merged, self.overrides, &mut sources)?;

        if require_source && sources.is_empty() {
            return Err(Error::ConfigNotFound);
        }

        let config = deserialize_config(merged, &sources)?;
        tracing::info!("configuration loaded");
        Ok((config, sources))
    }

    /// Load configuration, returning an error if no source is found.
    ///
    /// # Errors
    ///
    /// Returns [`Error::ConfigNotFound`] if no files, environment variables,
    /// or programmatic overrides supply configuration.
    pub fn load_or_error<C: serde::de::DeserializeOwned + Default + Serialize>(
        self,
    ) -> Result<(C, ConfigSources)> {
        self.load_inner(true)
    }

    /// Find project config by walking up from the given directory.
    fn find_project_config(&self, start: &Utf8Path) -> Option<Utf8PathBuf> {
        let candidates: Vec<_> = CONFIG_EXTENSIONS
            .iter()
            .map(|ext| {
                (
                    format!("{}.{ext}", self.app_name),
                    format!(".{}.{ext}", self.app_name),
                )
            })
            .collect();
        let read_entry_names = |directory: &Utf8Path| -> std::io::Result<HashSet<OsString>> {
            std::fs::read_dir(directory.as_std_path())?
                .map(|entry| entry.map(|entry| entry.file_name()))
                .collect()
        };
        let candidate_is_file =
            |entries: Option<&HashSet<OsString>>, directory: &Utf8Path, name: &str| match entries {
                Some(entries) if !entries.contains(OsStr::new(name)) => false,
                _ => directory.join(name).is_file(),
            };
        let mut current = Some(start.to_path_buf());

        while let Some(dir) = current {
            let root_entries = read_entry_names(&dir).ok();
            let dotconfig_dir = dir.join(".config");
            let dotconfig_entries = match root_entries.as_ref() {
                Some(entries) if entries.contains(OsStr::new(".config")) => {
                    read_entry_names(&dotconfig_dir).ok()
                }
                Some(_) => Some(HashSet::new()),
                None => None,
            };

            for (regular_name, dotfile_name) in &candidates {
                // .config/app.ext
                if candidate_is_file(dotconfig_entries.as_ref(), &dotconfig_dir, regular_name) {
                    return Some(dotconfig_dir.join(regular_name));
                }

                // .app.ext
                if candidate_is_file(root_entries.as_ref(), &dir, dotfile_name) {
                    return Some(dir.join(dotfile_name));
                }

                // app.ext
                if candidate_is_file(root_entries.as_ref(), &dir, regular_name) {
                    return Some(dir.join(regular_name));
                }
            }

            // Check boundary after checking config (so same-dir config is found)
            if let Some(ref marker) = self.boundary_marker
                && dir.join(marker).exists()
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

pub(crate) fn deserialize_config<C: serde::de::DeserializeOwned>(
    merged: Value,
    sources: &ConfigSources,
) -> Result<C> {
    serde_path_to_error::deserialize(merged).map_err(|error| {
        let path = match error.path().to_string().as_str() {
            "" | "." => "$".to_string(),
            path => path.to_string(),
        };
        let origin = sources
            .origin(&path)
            .cloned()
            .unwrap_or(ConfigOrigin::Default);
        Error::ConfigValue {
            path,
            origin,
            source: boxed_error(error.into_inner()),
        }
    })
}

pub(crate) fn apply_config_overrides(
    merged: &mut Value,
    overrides: Vec<ConfigOverride>,
    sources: &mut ConfigSources,
) -> Result<Vec<String>> {
    let mut paths = Vec::with_capacity(overrides.len());
    for config_override in overrides {
        let ConfigOverride { path, value } = config_override;
        let value = value.map_err(|error| Error::ConfigOverride {
            path: path.clone(),
            reason: error.to_string(),
        })?;
        let segments = override_path(&path)?;
        let mut origins = OriginMap::new();
        record_origins(
            &mut origins,
            &path,
            &value,
            ConfigOrigin::Override { path: path.clone() },
        );
        let mut overlay = Value::Object(serde_json::Map::new());
        set_path(&mut overlay, &segments, value);
        sources.record_merge_origins(merged, &overlay, &origins);
        deep_merge(merged, overlay)?;
        paths.push(path);
    }
    Ok(paths)
}

fn override_path(path: &str) -> Result<Vec<&str>> {
    let segments: Vec<&str> = path.split('.').collect();
    if segments.is_empty() || segments.iter().any(|segment| segment.is_empty()) {
        return Err(Error::ConfigOverride {
            path: path.to_string(),
            reason: "override path contains an empty segment".to_string(),
        });
    }
    if segments.len() > 64 {
        return Err(Error::ConfigOverride {
            path: path.to_string(),
            reason: "override path exceeds 64 levels".to_string(),
        });
    }
    Ok(segments)
}

fn set_path(target: &mut Value, path: &[&str], value: Value) {
    let Some((head, tail)) = path.split_first() else {
        return;
    };
    if tail.is_empty() {
        if !target.is_object() {
            *target = Value::Object(serde_json::Map::new());
        }
        target
            .as_object_mut()
            .expect("override target was initialized as an object")
            .insert((*head).to_string(), value);
        return;
    }

    if !target.is_object() {
        *target = Value::Object(serde_json::Map::new());
    }
    let child = target
        .as_object_mut()
        .expect("override target was initialized as an object")
        .entry((*head).to_string())
        .or_insert(Value::Null);
    set_path(child, tail, value);
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

/// Get the machine-local data directory for an application.
///
/// Unlike [`user_data_dir`], this location is not synchronized between a
/// user's machines. Use it for state that is meaningless elsewhere — caches
/// keyed by local paths, machine-specific identifiers, or logs.
///
/// On Linux and macOS this resolves to the same path as [`user_data_dir`];
/// the two diverge on Windows, where roaming and local application data are
/// genuinely different directories.
pub fn user_data_local_dir(app_name: &str) -> Option<Utf8PathBuf> {
    let proj_dirs = directories::ProjectDirs::from("", "", app_name)?;
    Utf8PathBuf::from_path_buf(proj_dirs.data_local_dir().to_path_buf()).ok()
}
