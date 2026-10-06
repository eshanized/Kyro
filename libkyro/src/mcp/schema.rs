use serde_json::Value;

/// Validates arguments against declared JSON schema of an MCP tool
pub fn validate_schema(schema: &Value, args: &Value) -> Result<(), String> {
    // If schema is null or empty object, args must be empty object or null
    if schema.is_null() || schema == &serde_json::json!({}) {
        return Ok(());
    }

    if !schema.is_object() {
        return Err("Declared inputSchema must be a JSON object".to_string());
    }

    let schema_type = schema
        .get("type")
        .and_then(|t| t.as_str())
        .unwrap_or("object");
    if schema_type != "object" {
        return Err(format!(
            "Unsupported root schema type '{}', expected 'object'",
            schema_type
        ));
    }

    let args_obj = match args.as_object() {
        Some(o) => o,
        None => return Err(format!("Expected JSON object arguments, found {:?}", args)),
    };

    // 1. Required fields validation
    if let Some(req_array) = schema.get("required").and_then(|r| r.as_array()) {
        for req in req_array {
            if let Some(req_field) = req.as_str() {
                if !args_obj.contains_key(req_field) {
                    return Err(format!("Missing required parameter '{}'", req_field));
                }
            }
        }
    }

    let properties = schema.get("properties").and_then(|p| p.as_object());
    let additional_props = schema
        .get("additionalProperties")
        .and_then(|a| a.as_bool())
        .unwrap_or(true);

    // 2. Validate declared properties
    for (key, val) in args_obj {
        if let Some(props) = properties {
            if let Some(prop_schema) = props.get(key) {
                validate_value(key, prop_schema, val)?;
                continue;
            }
        }
        if !additional_props {
            return Err(format!(
                "Unknown property '{}' is not permitted by schema",
                key
            ));
        }
    }

    Ok(())
}

fn validate_value(path: &str, schema: &Value, val: &Value) -> Result<(), String> {
    if let Some(expected_type) = schema.get("type").and_then(|t| t.as_str()) {
        match expected_type {
            "string" => {
                if !val.is_string() {
                    return Err(format!(
                        "Property '{}' must be a string, found {:?}",
                        path, val
                    ));
                }
                if let Some(min_len) = schema.get("minLength").and_then(|m| m.as_u64()) {
                    if (val.as_str().unwrap().len() as u64) < min_len {
                        return Err(format!(
                            "Property '{}' shorter than minLength {}",
                            path, min_len
                        ));
                    }
                }
            }
            "number" => {
                if !val.is_number() {
                    return Err(format!(
                        "Property '{}' must be a number, found {:?}",
                        path, val
                    ));
                }
            }
            "integer" => {
                if !val.is_i64() && !val.is_u64() {
                    return Err(format!(
                        "Property '{}' must be an integer, found {:?}",
                        path, val
                    ));
                }
            }
            "boolean" => {
                if !val.is_boolean() {
                    return Err(format!(
                        "Property '{}' must be a boolean, found {:?}",
                        path, val
                    ));
                }
            }
            "array" => {
                let arr = val.as_array().ok_or_else(|| {
                    format!("Property '{}' must be an array, found {:?}", path, val)
                })?;
                if let Some(item_schema) = schema.get("items") {
                    for (i, item) in arr.iter().enumerate() {
                        validate_value(&format!("{}[{}]", path, i), item_schema, item)?;
                    }
                }
            }
            "object" => {
                validate_schema(schema, val).map_err(|e| format!("{}: {}", path, e))?;
            }
            other => {
                return Err(format!("Unsupported schema type '{}' at '{}'", other, path));
            }
        }
    }

    // Enum validation
    if let Some(enum_vals) = schema.get("enum").and_then(|e| e.as_array()) {
        if !enum_vals.contains(val) {
            return Err(format!(
                "Property '{}' value {:?} is not one of permitted enum values: {:?}",
                path, val, enum_vals
            ));
        }
    }

    Ok(())
}
