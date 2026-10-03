//! Canonical JSON and SHA-256 helpers.
//!
//! Hashes in this repository are computed over canonical JSON: object keys are
//! sorted recursively and numbers use serde_json's shortest round-trip form. The
//! result does not depend on struct field order, map insertion order, or the
//! `preserve_order` feature of serde_json.

use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

/// Serialize any value to canonical JSON.
pub fn canonical_json_of<T: Serialize>(value: &T) -> Result<String, serde_json::Error> {
    let value = serde_json::to_value(value)?;
    Ok(canonical_json(&value))
}

/// Render a JSON value with recursively sorted object keys and no whitespace.
pub fn canonical_json(value: &Value) -> String {
    let mut out = String::new();
    write_canonical(value, &mut out);
    out
}

fn write_canonical(value: &Value, out: &mut String) {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push('{');
            for (index, key) in keys.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(&Value::String((*key).clone()).to_string());
                out.push(':');
                write_canonical(&map[*key], out);
            }
            out.push('}');
        }
        Value::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_canonical(item, out);
            }
            out.push(']');
        }
        other => out.push_str(&other.to_string()),
    }
}

/// `sha256:<hex>` digest of raw bytes.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(7 + digest.len() * 2);
    hex.push_str("sha256:");
    for byte in digest {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

/// `sha256:<hex>` digest of a value's canonical JSON.
pub fn hash_canonical<T: Serialize>(value: &T) -> Result<String, serde_json::Error> {
    Ok(sha256_hex(canonical_json_of(value)?.as_bytes()))
}

/// Round to a fixed number of decimal places so recorded floats stay legible
/// and comparisons are insensitive to representation noise. Non-finite values
/// are returned unchanged so callers can still detect and reject them.
pub fn quantize(value: f64, decimals: i32) -> f64 {
    if !value.is_finite() {
        return value;
    }
    let scale = 10f64.powi(decimals);
    let rounded = (value * scale).round() / scale;
    if rounded == 0.0 {
        0.0
    } else {
        rounded
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn key_order_does_not_change_canonical_form() {
        let left = json!({"b": 1, "a": {"y": [1, 2], "x": null}});
        let right = json!({"a": {"x": null, "y": [1, 2]}, "b": 1});
        assert_eq!(canonical_json(&left), canonical_json(&right));
        assert_eq!(canonical_json(&left), r#"{"a":{"x":null,"y":[1,2]},"b":1}"#);
    }

    #[test]
    fn strings_are_escaped() {
        let value = json!({"k\"": "line\nbreak"});
        assert_eq!(canonical_json(&value), r#"{"k\"":"line\nbreak"}"#);
    }

    #[test]
    fn sha256_has_prefix_and_length() {
        let hash = sha256_hex(b"resonate");
        assert!(hash.starts_with("sha256:"));
        assert_eq!(hash.len(), 7 + 64);
    }

    /// Representative doubles for the cross-language float-formatting
    /// contract: boundaries of ryu's layout rules plus pseudo-random values
    /// across the exponent range and short decimals like recorded metrics.
    fn float_cases() -> Vec<f64> {
        let mut cases = vec![
            0.0,
            -0.0,
            1.0,
            -1.0,
            0.1,
            0.5,
            1.5,
            100.0,
            1e15,
            1e16,
            1e17,
            9007199254740993.0,
            123456789012345680.0,
            1e-4,
            1e-5,
            1e-6,
            1e-7,
            0.000123,
            1.25e-7,
            5e-324,
            f64::MIN_POSITIVE,
            f64::MAX,
            f64::EPSILON,
            1700000000000.0,
            0.30000000000000004,
            2.0f64.powi(60),
        ];
        for exponent in -20..=20 {
            cases.push(10f64.powi(exponent));
            cases.push(-3.75 * 10f64.powi(exponent));
        }
        let mut state: u64 = 0x05ee_d0ff_10a7;
        let mut next = || {
            state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = state;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        };
        for _ in 0..200 {
            let value = f64::from_bits(next());
            if value.is_finite() {
                cases.push(value);
            }
        }
        for _ in 0..200 {
            let mantissa = (next() % 2_000_000) as f64 - 1_000_000.0;
            cases.push(quantize(mantissa / 10f64.powi((next() % 9) as i32), 6));
        }
        cases
    }

    /// `fixtures/canonical/floats.json` pins serde_json's float rendering so
    /// the independent Python verifier can reproduce canonical JSON exactly.
    /// Regenerate with `UPDATE_FIXTURES=1 cargo test -p event-bus`.
    #[test]
    fn float_rendering_matches_the_cross_language_fixture() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/canonical/floats.json");
        let rendered: Vec<Value> = float_cases()
            .into_iter()
            .map(|value| {
                json!({
                    "bits": format!("{:016x}", value.to_bits()),
                    "json": canonical_json(&json!(value)),
                })
            })
            .collect();
        let text = serde_json::to_string_pretty(&rendered).expect("serializes") + "\n";
        if std::env::var_os("UPDATE_FIXTURES").is_some() {
            std::fs::create_dir_all(path.parent().expect("has parent")).expect("mkdir");
            std::fs::write(&path, &text).expect("write fixture");
        }
        let committed = std::fs::read_to_string(&path).expect("fixture exists");
        assert_eq!(
            committed,
            text,
            "float rendering changed; see {}",
            path.display()
        );
    }

    #[test]
    fn quantize_rounds_and_normalizes_negative_zero() {
        assert_eq!(quantize(1.234_567_89, 4), 1.2346);
        assert_eq!(quantize(-0.000_000_1, 4).to_bits(), 0.0f64.to_bits());
        assert!(quantize(f64::NAN, 4).is_nan());
    }
}
