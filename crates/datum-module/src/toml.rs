//! Minimal TOML subset used by module manifests and installation profiles.
//!
//! No third-party TOML crate is on the workspace allow-list. This parser covers
//! the documents this crate ships: tables, array-of-tables, dotted keys,
//! strings, booleans, integers, and arrays of those scalars.

use std::collections::BTreeMap;

use crate::{Error, Result};

/// A TOML value.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Value {
    String(String),
    Boolean(bool),
    Integer(i64),
    Array(Vec<Value>),
    Table(BTreeMap<String, Value>),
}

impl Value {
    pub(crate) fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    pub(crate) fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Boolean(b) => Some(*b),
            _ => None,
        }
    }

    pub(crate) fn as_integer(&self) -> Option<i64> {
        match self {
            Value::Integer(i) => Some(*i),
            _ => None,
        }
    }

    pub(crate) fn as_array(&self) -> Option<&[Value]> {
        match self {
            Value::Array(a) => Some(a),
            _ => None,
        }
    }

    pub(crate) fn as_table(&self) -> Option<&BTreeMap<String, Value>> {
        match self {
            Value::Table(t) => Some(t),
            _ => None,
        }
    }

    pub(crate) fn table_mut(&mut self) -> Option<&mut BTreeMap<String, Value>> {
        match self {
            Value::Table(t) => Some(t),
            _ => None,
        }
    }
}

/// Parse a document into a root table.
pub(crate) fn parse(input: &str) -> Result<BTreeMap<String, Value>> {
    let mut root: BTreeMap<String, Value> = BTreeMap::new();
    let mut current_path: Vec<String> = Vec::new();
    let mut array_table = false;
    let mut skip_until = 0usize;
    for (idx, raw) in input.lines().enumerate() {
        if idx < skip_until {
            continue;
        }
        let line_no = idx + 1;
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        if let Some(inner) = line.strip_prefix("[[") {
            let Some(end) = inner.strip_suffix("]]") else {
                return Err(err(line_no, "unclosed array-of-tables"));
            };
            current_path = split_path(end.trim())?;
            array_table = true;
            push_array_table(&mut root, &current_path)?;
            continue;
        }
        if let Some(inner) = line.strip_prefix('[') {
            let Some(end) = inner.strip_suffix(']') else {
                return Err(err(line_no, "unclosed table"));
            };
            current_path = split_path(end.trim())?;
            array_table = false;
            ensure_table(&mut root, &current_path)?;
            continue;
        }
        let Some((key, rest)) = line.split_once('=') else {
            return Err(err(line_no, "expected key = value"));
        };
        let key = key.trim();
        let key = key
            .strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
            .unwrap_or(key);
        if key.is_empty() {
            return Err(err(line_no, "empty key"));
        }
        let mut value_src = rest.trim().to_string();
        if value_src.starts_with('[') && !array_closed(&value_src) {
            for (j, more) in input.lines().enumerate().skip(idx + 1) {
                let extra = strip_comment(more).trim();
                value_src.push(' ');
                value_src.push_str(extra);
                if array_closed(&value_src) {
                    skip_until = j + 1;
                    break;
                }
            }
        }
        let value = parse_value(&value_src, line_no)?;
        let path = if current_path.is_empty() {
            vec![key.to_string()]
        } else {
            let mut p = current_path.clone();
            p.push(key.to_string());
            p
        };
        insert_key(&mut root, &path, value, array_table)?;
    }
    Ok(root)
}

fn err(line: usize, msg: &str) -> Error {
    Error::Toml(format!("line {line}: {msg}"))
}

fn strip_comment(line: &str) -> &str {
    let mut in_str = false;
    for (i, c) in line.char_indices() {
        match c {
            '"' if !in_str => in_str = true,
            '"' if in_str => in_str = false,
            '#' if !in_str => return &line[..i],
            _ => {}
        }
    }
    line
}

fn split_path(path: &str) -> Result<Vec<String>> {
    if path.is_empty() {
        return Err(Error::Toml("empty table path".into()));
    }
    Ok(path.split('.').map(|s| s.trim().to_string()).collect())
}

fn ensure_table(root: &mut BTreeMap<String, Value>, path: &[String]) -> Result<()> {
    let mut cur = root;
    for key in path {
        let entry = cur
            .entry(key.clone())
            .or_insert_with(|| Value::Table(BTreeMap::new()));
        cur = match entry {
            Value::Table(t) => t,
            Value::Array(arr) => {
                let last = arr
                    .last_mut()
                    .ok_or_else(|| Error::Toml(format!("array {key} has no table to extend")))?;
                last.table_mut()
                    .ok_or_else(|| Error::Toml(format!("{key} is not a table")))?
            }
            _ => return Err(Error::Toml(format!("{key} is not a table"))),
        };
    }
    Ok(())
}

fn push_array_table(root: &mut BTreeMap<String, Value>, path: &[String]) -> Result<()> {
    let Some((last, parents)) = path.split_last() else {
        return Err(Error::Toml("empty array-of-tables path".into()));
    };
    let mut cur = root;
    for key in parents {
        let entry = cur
            .entry(key.clone())
            .or_insert_with(|| Value::Table(BTreeMap::new()));
        cur = match entry {
            Value::Table(t) => t,
            Value::Array(arr) => {
                let last = arr
                    .last_mut()
                    .ok_or_else(|| Error::Toml(format!("array {key} has no table to extend")))?;
                last.table_mut()
                    .ok_or_else(|| Error::Toml(format!("{key} is not a table")))?
            }
            _ => return Err(Error::Toml(format!("{key} is not a table"))),
        };
    }
    let entry = cur
        .entry(last.clone())
        .or_insert_with(|| Value::Array(Vec::new()));
    match entry {
        Value::Array(arr) => arr.push(Value::Table(BTreeMap::new())),
        _ => return Err(Error::Toml(format!("{last} is not an array"))),
    }
    Ok(())
}

fn insert_key(
    root: &mut BTreeMap<String, Value>,
    path: &[String],
    value: Value,
    array_table: bool,
) -> Result<()> {
    let Some((last, parents)) = path.split_last() else {
        return Err(Error::Toml("empty key path".into()));
    };
    let mut cur = root;
    for (i, key) in parents.iter().enumerate() {
        let in_array_head = array_table && i + 1 == parents.len();
        let entry = cur.entry(key.clone()).or_insert_with(|| {
            if in_array_head {
                Value::Array(vec![Value::Table(BTreeMap::new())])
            } else {
                Value::Table(BTreeMap::new())
            }
        });
        cur = match entry {
            Value::Table(t) => t,
            Value::Array(arr) => {
                if let Some(Value::Table(t)) = arr.last_mut() {
                    t
                } else {
                    return Err(Error::Toml(format!("{key} array has no table")));
                }
            }
            _ => return Err(Error::Toml(format!("{key} is not a table"))),
        };
    }
    cur.insert(last.clone(), value);
    Ok(())
}

fn array_closed(s: &str) -> bool {
    let mut depth = 0i32;
    let mut in_str = false;
    for c in s.chars() {
        match c {
            '"' => in_str = !in_str,
            '[' if !in_str => depth += 1,
            ']' if !in_str => depth -= 1,
            _ => {}
        }
    }
    depth == 0 && s.contains(']')
}

fn parse_value(s: &str, line: usize) -> Result<Value> {
    if s == "true" {
        return Ok(Value::Boolean(true));
    }
    if s == "false" {
        return Ok(Value::Boolean(false));
    }
    if let Some(inner) = s.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
        return Ok(Value::String(unescape(inner)));
    }
    if let Some(inner) = s.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')) {
        return Ok(Value::String(inner.to_string()));
    }
    if let Some(inner) = s.strip_prefix('[') {
        let Some(inner) = inner.strip_suffix(']') else {
            return Err(err(line, "unclosed array"));
        };
        return Ok(Value::Array(parse_array(inner, line)?));
    }
    if let Ok(i) = s.parse::<i64>() {
        return Ok(Value::Integer(i));
    }
    Err(err(line, &format!("unrecognized value {s}")))
}

fn parse_array(inner: &str, line: usize) -> Result<Vec<Value>> {
    let inner = inner.trim();
    if inner.is_empty() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut in_str = false;
    for c in inner.chars() {
        match c {
            '"' => {
                in_str = !in_str;
                buf.push(c);
            }
            ',' if !in_str => {
                let item = buf.trim();
                if !item.is_empty() {
                    out.push(parse_value(item, line)?);
                }
                buf.clear();
            }
            _ => buf.push(c),
        }
    }
    let item = buf.trim();
    if !item.is_empty() {
        out.push(parse_value(item, line)?);
    }
    Ok(out)
}

fn unescape(s: &str) -> String {
    s.replace("\\\"", "\"").replace("\\\\", "\\")
}

pub(crate) fn require_table<'a>(
    table: &'a BTreeMap<String, Value>,
    key: &str,
) -> Result<&'a BTreeMap<String, Value>> {
    table
        .get(key)
        .and_then(Value::as_table)
        .ok_or_else(|| Error::Toml(format!("missing table [{key}]")))
}

pub(crate) fn require_str(table: &BTreeMap<String, Value>, key: &str) -> Result<String> {
    table
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| Error::Toml(format!("missing string {key}")))
}

pub(crate) fn require_bool(table: &BTreeMap<String, Value>, key: &str) -> Result<bool> {
    table
        .get(key)
        .and_then(Value::as_bool)
        .ok_or_else(|| Error::Toml(format!("missing bool {key}")))
}

pub(crate) fn optional_str(table: &BTreeMap<String, Value>, key: &str) -> Result<Option<String>> {
    match table.get(key) {
        None => Ok(None),
        Some(v) => v
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| Error::Toml(format!("{key} must be a string")))
            .map(Some),
    }
}

pub(crate) fn optional_int(table: &BTreeMap<String, Value>, key: &str) -> Result<Option<i64>> {
    match table.get(key) {
        None => Ok(None),
        Some(v) => v
            .as_integer()
            .ok_or_else(|| Error::Toml(format!("{key} must be an integer")))
            .map(Some),
    }
}

pub(crate) fn string_array(value: &Value) -> Result<Vec<String>> {
    let Some(arr) = value.as_array() else {
        return Err(Error::Toml("expected string array".into()));
    };
    let mut out = Vec::new();
    for v in arr {
        match v {
            Value::String(s) => out.push(s.clone()),
            Value::Integer(i) => out.push(i.to_string()),
            _ => return Err(Error::Toml("array item is not a string".into())),
        }
    }
    Ok(out)
}

pub(crate) fn int_array(value: &Value) -> Result<Vec<i64>> {
    let Some(arr) = value.as_array() else {
        return Err(Error::Toml("expected integer array".into()));
    };
    let mut out = Vec::new();
    for v in arr {
        out.push(
            v.as_integer()
                .ok_or_else(|| Error::Toml("array item is not an integer".into()))?,
        );
    }
    Ok(out)
}
