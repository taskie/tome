//! Helper functions for converting between DynamoDB AttributeValue and Rust types.

#![allow(dead_code)]

use std::collections::HashMap;

use aws_sdk_dynamodb::types::AttributeValue;
use chrono::{DateTime, FixedOffset, Utc};

pub type Item = HashMap<String, AttributeValue>;

// ── Getters ──────────────────────────────────────────────────────────────────

pub fn get_s(item: &Item, key: &str) -> anyhow::Result<String> {
    item.get(key)
        .and_then(|v| v.as_s().ok())
        .map(|s| s.to_owned())
        .ok_or_else(|| anyhow::anyhow!("missing string attribute: {key}"))
}

pub fn get_s_opt(item: &Item, key: &str) -> Option<String> {
    item.get(key).and_then(|v| v.as_s().ok()).map(|s| s.to_owned())
}

pub fn get_n_i64(item: &Item, key: &str) -> anyhow::Result<i64> {
    item.get(key)
        .and_then(|v| v.as_n().ok())
        .and_then(|n| n.parse::<i64>().ok())
        .ok_or_else(|| anyhow::anyhow!("missing numeric attribute: {key}"))
}

pub fn get_n_i64_opt(item: &Item, key: &str) -> Option<i64> {
    item.get(key).and_then(|v| v.as_n().ok()).and_then(|n| n.parse::<i64>().ok())
}

pub fn get_n_i16(item: &Item, key: &str) -> anyhow::Result<i16> {
    item.get(key)
        .and_then(|v| v.as_n().ok())
        .and_then(|n| n.parse::<i16>().ok())
        .ok_or_else(|| anyhow::anyhow!("missing numeric attribute: {key}"))
}

pub fn get_n_i16_opt(item: &Item, key: &str) -> Option<i16> {
    item.get(key).and_then(|v| v.as_n().ok()).and_then(|n| n.parse::<i16>().ok())
}

pub fn get_n_i32_opt(item: &Item, key: &str) -> Option<i32> {
    item.get(key).and_then(|v| v.as_n().ok()).and_then(|n| n.parse::<i32>().ok())
}

pub fn get_n_u64(item: &Item, key: &str) -> anyhow::Result<u64> {
    item.get(key)
        .and_then(|v| v.as_n().ok())
        .and_then(|n| n.parse::<u64>().ok())
        .ok_or_else(|| anyhow::anyhow!("missing numeric attribute: {key}"))
}

pub fn get_bool(item: &Item, key: &str) -> anyhow::Result<bool> {
    item.get(key)
        .and_then(|v| v.as_bool().ok())
        .copied()
        .ok_or_else(|| anyhow::anyhow!("missing bool attribute: {key}"))
}

pub fn get_bytes(item: &Item, key: &str) -> anyhow::Result<Vec<u8>> {
    item.get(key)
        .and_then(|v| v.as_b().ok())
        .map(|b| b.as_ref().to_vec())
        .ok_or_else(|| anyhow::anyhow!("missing binary attribute: {key}"))
}

pub fn get_bytes_opt(item: &Item, key: &str) -> Option<Vec<u8>> {
    item.get(key).and_then(|v| v.as_b().ok()).map(|b| b.as_ref().to_vec())
}

pub fn get_json(item: &Item, key: &str) -> anyhow::Result<serde_json::Value> {
    let s = get_s(item, key)?;
    Ok(serde_json::from_str(&s)?)
}

pub fn get_json_or_null(item: &Item, key: &str) -> serde_json::Value {
    get_s_opt(item, key).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(serde_json::Value::Null)
}

pub fn get_datetime(item: &Item, key: &str) -> anyhow::Result<DateTime<FixedOffset>> {
    let s = get_s(item, key)?;
    Ok(s.parse::<DateTime<FixedOffset>>()?)
}

pub fn get_datetime_opt(item: &Item, key: &str) -> Option<DateTime<FixedOffset>> {
    get_s_opt(item, key).and_then(|s| s.parse::<DateTime<FixedOffset>>().ok())
}

// ── Setters ──────────────────────────────────────────────────────────────────

pub fn s(val: &str) -> AttributeValue {
    AttributeValue::S(val.to_owned())
}

pub fn n_i64(val: i64) -> AttributeValue {
    AttributeValue::N(val.to_string())
}

pub fn n_i16(val: i16) -> AttributeValue {
    AttributeValue::N(val.to_string())
}

pub fn n_i32(val: i32) -> AttributeValue {
    AttributeValue::N(val.to_string())
}

pub fn n_u64(val: u64) -> AttributeValue {
    AttributeValue::N(val.to_string())
}

pub fn b(val: &[u8]) -> AttributeValue {
    AttributeValue::B(aws_sdk_dynamodb::primitives::Blob::new(val))
}

pub fn bool_val(val: bool) -> AttributeValue {
    AttributeValue::Bool(val)
}

pub fn json_val(val: &serde_json::Value) -> AttributeValue {
    AttributeValue::S(val.to_string())
}

pub fn now_iso() -> String {
    Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap()).to_rfc3339()
}

pub fn datetime_iso(dt: &DateTime<FixedOffset>) -> String {
    dt.to_rfc3339()
}
