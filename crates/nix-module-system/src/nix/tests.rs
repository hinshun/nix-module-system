//! Tests for the Nix evaluation module.

use super::*;
use crate::types::Value;
use std::path::PathBuf;

#[test]
fn test_json_conversion() {
    let json = serde_json::json!({
        "services": {
            "nginx": {
                "enable": true,
                "port": 80
            }
        },
        "users": ["alice", "bob"]
    });

    let value = json_to_value(&json);

    // Check structure
    if let Value::Attrs(attrs) = &value {
        assert!(attrs.contains_key("services"));
        assert!(attrs.contains_key("users"));

        if let Value::Attrs(services) = attrs.get("services").unwrap() {
            if let Value::Attrs(nginx) = services.get("nginx").unwrap() {
                assert_eq!(nginx.get("enable"), Some(&Value::Bool(true)));
                assert_eq!(nginx.get("port"), Some(&Value::Int(80)));
            } else {
                panic!("Expected nginx to be attrs");
            }
        } else {
            panic!("Expected services to be attrs");
        }

        if let Value::List(users) = attrs.get("users").unwrap() {
            assert_eq!(users.len(), 2);
            assert_eq!(users[0], Value::String("alice".to_string()));
            assert_eq!(users[1], Value::String("bob".to_string()));
        } else {
            panic!("Expected users to be list");
        }
    } else {
        panic!("Expected root to be attrs");
    }
}

#[test]
fn test_error_types() {
    // Test different error constructors
    let parse_err = NixError::parse("/test.nix", "unexpected token");
    assert!(matches!(parse_err, NixError::ParseError { .. }));

    let eval_err = NixError::evaluation("undefined variable 'x'");
    assert!(matches!(eval_err, NixError::EvaluationError { .. }));

    let type_err = NixError::type_error("string", "int");
    assert!(matches!(type_err, NixError::TypeError { .. }));

    let conv_err = NixError::conversion("cannot convert lambda");
    assert!(matches!(conv_err, NixError::ConversionError { .. }));
}

#[test]
fn test_error_recoverable() {
    let recoverable = NixError::AttributeNotFound {
        name: "foo".to_string(),
        path: "config".to_string(),
    };
    assert!(recoverable.is_recoverable());

    let not_recoverable = NixError::evaluation("fatal error");
    assert!(!not_recoverable.is_recoverable());
}

#[test]
fn test_trace_frame() {
    let frame = TraceFrame::new("while evaluating 'foo'");
    assert_eq!(frame.description, "while evaluating 'foo'");
    assert!(frame.file.is_none());

    let frame_with_loc = frame.with_location(PathBuf::from("/test.nix"), 10, 5);
    assert_eq!(frame_with_loc.file, Some(PathBuf::from("/test.nix")));
    assert_eq!(frame_with_loc.line, Some(10));
    assert_eq!(frame_with_loc.column, Some(5));
}

#[test]
fn test_config_defaults() {
    let config = NixConfig::default();
    assert!(config.store_uri.is_none());
    assert!(config.lookup_paths.is_empty());
    assert!(!config.allow_impure);
    assert!(!config.enable_trace);
    assert_eq!(config.max_depth, 0);
}

#[test]
fn test_value_json_special_cases() {
    // Test path conversion
    let path_value = Value::Path(PathBuf::from("/nix/store/hash-name"));
    let json = value_to_json(&path_value);
    assert_eq!(json.as_str(), Some("/nix/store/hash-name"));

    // Test lambda conversion
    let lambda_value = Value::Lambda;
    let json = value_to_json(&lambda_value);
    assert_eq!(json.as_str(), Some("<lambda>"));

    // Test derivation conversion
    let drv_attrs = Value::Attrs(
        [("name".to_string(), Value::String("test".to_string()))]
            .into_iter()
            .collect(),
    );
    let drv_value = Value::Derivation(Box::new(drv_attrs));
    let json = value_to_json(&drv_value);
    assert!(json.is_object());
}
