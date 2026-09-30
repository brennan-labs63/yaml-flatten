// Turns a parsed document into dotted key/value pairs, e.g. a `database.host`
// key nested three levels deep becomes "database.host=localhost". Sequence
// indices are appended as "[i]" so paths stay unambiguous and stable.

use crate::parser::Value;

pub fn flatten(value: &Value) -> Vec<(String, String)> {
    walk(value, "")
}

fn walk(value: &Value, prefix: &str) -> Vec<(String, String)> {
    // An empty `{}` or `[]` has no leaves, but dropping it would hide that the
    // key exists at all, which matters when diffing two files.
    match value {
        Value::Mapping(entries) if entries.is_empty() && !prefix.is_empty() => {
            return vec![(prefix.to_string(), "{}".to_string())];
        }
        Value::Sequence(items) if items.is_empty() && !prefix.is_empty() => {
            return vec![(prefix.to_string(), "[]".to_string())];
        }
        _ => {}
    }
    match value {
        Value::Mapping(entries) => entries
            .iter()
            .flat_map(|(key, v)| {
                let path = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                walk(v, &path)
            })
            .collect(),
        Value::Sequence(items) => items
            .iter()
            .enumerate()
            .flat_map(|(i, v)| walk(v, &format!("{prefix}[{i}]")))
            .collect(),
        other => vec![(prefix.to_string(), scalar_to_string(other))],
    }
}

pub fn scalar_to_string(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => format_number(*n),
        Value::String(s) => s.clone(),
        Value::Mapping(_) | Value::Sequence(_) => String::new(),
    }
}

fn format_number(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        n.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flattens_nested_structure() {
        let value = Value::Mapping(vec![(
            "service".to_string(),
            Value::Mapping(vec![
                ("name".to_string(), Value::String("demo".to_string())),
                ("port".to_string(), Value::Number(8080.0)),
            ]),
        )]);
        assert_eq!(
            flatten(&value),
            vec![
                ("service.name".to_string(), "demo".to_string()),
                ("service.port".to_string(), "8080".to_string()),
            ]
        );
    }

    #[test]
    fn flattens_sequences_with_index_paths() {
        let value = Value::Mapping(vec![(
            "hosts".to_string(),
            Value::Sequence(vec![
                Value::String("a".to_string()),
                Value::String("b".to_string()),
            ]),
        )]);
        assert_eq!(
            flatten(&value),
            vec![
                ("hosts[0]".to_string(), "a".to_string()),
                ("hosts[1]".to_string(), "b".to_string()),
            ]
        );
    }

    #[test]
    fn keeps_empty_collections_visible() {
        let value = Value::Mapping(vec![
            ("labels".to_string(), Value::Mapping(vec![])),
            ("hosts".to_string(), Value::Sequence(vec![])),
        ]);
        assert_eq!(
            flatten(&value),
            vec![
                ("labels".to_string(), "{}".to_string()),
                ("hosts".to_string(), "[]".to_string()),
            ]
        );
    }

    #[test]
    fn formats_whole_numbers_without_decimal() {
        assert_eq!(format_number(8080.0), "8080");
        assert_eq!(format_number(3.5), "3.5");
    }
}
