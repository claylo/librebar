use std::ffi::OsString;

use serde_json::{Map, Value};

use crate::config::{ConfigOrigin, OriginMap, record_origins};
use crate::error::BoxError;
use crate::{Error, Result};

const ENV_PATH_DEPTH_LIMIT: usize = 64;

/// Source of process-style configuration variables.
pub trait EnvironmentSource {
    /// Return environment key/value pairs matching `prefix`.
    ///
    /// The prefix is the normalized uppercase application name with a trailing
    /// underscore, such as `MY_APP_`. Implementations may use it to avoid
    /// querying or returning unrelated values.
    ///
    /// # Errors
    ///
    /// Returns an error when the backing source cannot be queried.
    fn vars(&self, prefix: &str) -> std::result::Result<Vec<(OsString, OsString)>, BoxError>;
}

/// The current process environment.
#[derive(Debug, Clone, Copy, Default)]
pub struct ProcessEnvironment;

impl EnvironmentSource for ProcessEnvironment {
    fn vars(&self, prefix: &str) -> std::result::Result<Vec<(OsString, OsString)>, BoxError> {
        Ok(std::env::vars_os()
            .filter(|(key, _)| key.to_str().is_some_and(|key| key.starts_with(prefix)))
            .collect())
    }
}

/// Policy for prefixed variables whose paths are absent from lower layers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum UnknownEnvironment {
    /// Ignore unknown paths without decoding their values.
    #[default]
    Ignore,
    /// Insert unknown paths as strings.
    Collect,
}

pub(super) fn overlay(
    app_name: &str,
    schema: &Value,
    source: &dyn EnvironmentSource,
    unknown: UnknownEnvironment,
) -> Result<(Value, Vec<String>, OriginMap)> {
    let prefix = prefix(app_name);
    let mut overlay = Map::new();
    let mut applied = Vec::new();
    let mut origins = OriginMap::new();

    let mut variables: Vec<(String, OsString)> = source
        .vars(&prefix)
        .map_err(Error::ConfigEnvironmentSource)?
        .into_iter()
        .filter_map(|(key, value)| key.into_string().ok().map(|key| (key, value)))
        .filter(|(key, _)| key.starts_with(&prefix))
        .collect();
    variables.sort_by(|left, right| left.0.cmp(&right.0));

    for (key, value) in variables {
        let path = path(&key, &prefix)?;
        let value_schema = schema_at(schema, &path);
        let known = value_schema.is_some();
        if !known && unknown == UnknownEnvironment::Ignore {
            continue;
        }
        let value = value
            .into_string()
            .map_err(|_| environment_error(&key, "value is not valid UTF-8"))?;
        let value = coerce(&key, value, value_schema)?;

        let config_path = path.join(".");
        record_origins(
            &mut origins,
            &config_path,
            &value,
            ConfigOrigin::Environment {
                variable: key.clone(),
            },
        );
        insert(&mut overlay, &path, value, &key)?;
        applied.push(key);
    }

    Ok((Value::Object(overlay), applied, origins))
}

fn path(variable: &str, prefix: &str) -> Result<Vec<String>> {
    let suffix = variable
        .strip_prefix(prefix)
        .expect("environment variables were filtered by prefix");
    let path: Vec<String> = suffix.split("__").map(str::to_ascii_lowercase).collect();
    if path.is_empty() || path.iter().any(String::is_empty) {
        return Err(environment_error(
            variable,
            "environment path contains an empty segment",
        ));
    }
    if path.len() > ENV_PATH_DEPTH_LIMIT {
        return Err(environment_error(
            variable,
            "environment path exceeds 64 levels",
        ));
    }
    Ok(path)
}

fn coerce(variable: &str, raw: String, schema: Option<&Value>) -> Result<Value> {
    match schema {
        Some(Value::Bool(_)) => match raw.as_str() {
            "true" => Ok(Value::Bool(true)),
            "false" => Ok(Value::Bool(false)),
            _ => Err(environment_error(variable, "expected `true` or `false`")),
        },
        Some(Value::Number(number)) if number.is_i64() => raw
            .parse::<i64>()
            .map(Value::from)
            .map_err(|_| environment_error(variable, "expected a signed integer")),
        Some(Value::Number(number)) if number.is_u64() => raw
            .parse::<u64>()
            .map(Value::from)
            .map_err(|_| environment_error(variable, "expected an unsigned integer")),
        Some(Value::Number(_)) => raw
            .parse::<f64>()
            .ok()
            .and_then(serde_json::Number::from_f64)
            .map(Value::Number)
            .ok_or_else(|| environment_error(variable, "expected a finite number")),
        Some(Value::Array(_)) => parse_compound(variable, &raw, "array", Value::is_array),
        Some(Value::Object(_)) => parse_compound(variable, &raw, "object", Value::is_object),
        Some(Value::String(_) | Value::Null) | None => Ok(Value::String(raw)),
    }
}

fn parse_compound(
    variable: &str,
    raw: &str,
    expected: &str,
    matches: impl FnOnce(&Value) -> bool,
) -> Result<Value> {
    let value: Value = serde_json::from_str(raw)
        .map_err(|_| environment_error(variable, &format!("expected a JSON {expected}")))?;
    if matches(&value) {
        Ok(value)
    } else {
        Err(environment_error(
            variable,
            &format!("expected a JSON {expected}"),
        ))
    }
}

fn environment_error(variable: &str, reason: &str) -> crate::Error {
    crate::Error::ConfigEnvironment {
        variable: variable.to_string(),
        reason: reason.to_string(),
    }
}

fn prefix(app_name: &str) -> String {
    let mut result: String = app_name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect();
    result.push('_');
    result
}

fn schema_at<'a>(schema: &'a Value, path: &[String]) -> Option<&'a Value> {
    path.iter()
        .try_fold(schema, |value, segment| value.as_object()?.get(segment))
}

fn insert(
    target: &mut Map<String, Value>,
    path: &[String],
    value: Value,
    variable: &str,
) -> Result<()> {
    let Some((head, tail)) = path.split_first() else {
        return Err(environment_error(variable, "environment path is empty"));
    };
    if tail.is_empty() {
        if target.contains_key(head) {
            return Err(environment_error(
                variable,
                "environment path duplicates or conflicts with another variable",
            ));
        }
        target.insert(head.clone(), value);
        return Ok(());
    }

    let child = target
        .entry(head.clone())
        .or_insert_with(|| Value::Object(Map::new()));
    let Some(child) = child.as_object_mut() else {
        return Err(environment_error(
            variable,
            "environment path conflicts with a parent variable",
        ));
    };
    insert(child, tail, value, variable)
}
