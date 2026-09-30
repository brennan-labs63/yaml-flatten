// Parses the block-style subset of YAML actually seen in config files: nested
// mappings, sequences, and scalars, with comments and blank lines ignored.
// Flow collections ({a: 1}, [1, 2]) are accepted as values when they fit on
// one line. No anchors, aliases, or multi-line block scalars yet - those are
// rare in hand-written config and can be added when something actually needs
// them.

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Mapping(Vec<(String, Value)>),
    Sequence(Vec<Value>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub line: usize,
    pub message: String,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

#[derive(Debug, Clone, Copy)]
struct Line<'a> {
    number: usize,
    indent: usize,
    content: &'a str,
}

pub fn parse(input: &str) -> Result<Value, ParseError> {
    let lines = tokenize(input);
    let first = match lines.first() {
        Some(l) => l,
        None => return Ok(Value::Null),
    };
    let (value, rest) = parse_block(&lines, first.indent)?;
    if let Some(extra) = rest.first() {
        return Err(ParseError {
            line: extra.number,
            message: "unexpected indentation".to_string(),
        });
    }
    Ok(value)
}

fn tokenize(input: &str) -> Vec<Line> {
    input
        .lines()
        .enumerate()
        .filter_map(|(i, raw)| {
            let stripped = strip_comment(raw);
            let trimmed = stripped.trim_end();
            let content = trimmed.trim_start();
            if content.is_empty() || content == "---" || content == "..." {
                return None;
            }
            let indent = trimmed.len() - content.len();
            Some(Line {
                number: i + 1,
                indent,
                content,
            })
        })
        .collect()
}

fn strip_comment(line: &str) -> &str {
    let bytes = line.as_bytes();
    let mut in_single = false;
    let mut in_double = false;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'\'' if !in_double => in_single = !in_single,
            b'"' if !in_single => in_double = !in_double,
            b'#' if !in_single && !in_double => {
                if i == 0 || bytes[i - 1] == b' ' || bytes[i - 1] == b'\t' {
                    return &line[..i];
                }
            }
            _ => {}
        }
    }
    line
}

fn parse_block<'a>(
    lines: &[Line<'a>],
    indent: usize,
) -> Result<(Value, &[Line<'a>]), ParseError> {
    let first = match lines.first() {
        Some(l) => l,
        None => return Ok((Value::Null, lines)),
    };
    if first.indent != indent {
        return Err(ParseError {
            line: first.number,
            message: "unexpected indentation".to_string(),
        });
    }
    if is_sequence_item(first.content) {
        parse_sequence(lines, indent)
    } else {
        parse_mapping(lines, indent)
    }
}

fn is_sequence_item(content: &str) -> bool {
    content == "-" || content.starts_with("- ")
}

fn parse_sequence<'a>(
    lines: &[Line<'a>],
    indent: usize,
) -> Result<(Value, &[Line<'a>]), ParseError> {
    let mut items = Vec::new();
    let mut rest = lines;
    while let Some(line) = rest.first() {
        if line.indent != indent || !is_sequence_item(line.content) {
            break;
        }
        let item_content = if line.content == "-" {
            ""
        } else {
            line.content[2..].trim_start()
        };
        if item_content.is_empty() {
            let after = &rest[1..];
            match after.first() {
                Some(next) if next.indent > indent => {
                    let (value, remaining) = parse_block(after, next.indent)?;
                    items.push(value);
                    rest = remaining;
                }
                _ => {
                    items.push(Value::Null);
                    rest = after;
                }
            }
        } else if is_flow(item_content) {
            items.push(parse_inline(item_content, line.number)?);
            rest = &rest[1..];
        } else if is_sequence_item(item_content) || find_key_separator(item_content).is_some() {
            let virtual_indent = line.indent + 2;
            let mut group = vec![Line {
                number: line.number,
                indent: virtual_indent,
                content: item_content,
            }];
            let consumed = 1 + collect_continuation(&rest[1..], indent, &mut group);
            let (value, leftover) = parse_block(&group, virtual_indent)?;
            ensure_consumed(leftover)?;
            items.push(value);
            rest = &rest[consumed..];
        } else {
            items.push(parse_scalar(item_content));
            rest = &rest[1..];
        }
    }
    Ok((Value::Sequence(items), rest))
}

fn collect_continuation<'a>(
    lines: &[Line<'a>],
    base_indent: usize,
    group: &mut Vec<Line<'a>>,
) -> usize {
    let mut taken = 0;
    for line in lines {
        if line.indent > base_indent {
            group.push(*line);
            taken += 1;
        } else {
            break;
        }
    }
    taken
}

fn ensure_consumed(leftover: &[Line]) -> Result<(), ParseError> {
    match leftover.first() {
        Some(line) => Err(ParseError {
            line: line.number,
            message: "unexpected indentation".to_string(),
        }),
        None => Ok(()),
    }
}

fn parse_mapping<'a>(
    lines: &[Line<'a>],
    indent: usize,
) -> Result<(Value, &[Line<'a>]), ParseError> {
    let mut entries = Vec::new();
    let mut rest = lines;
    while let Some(line) = rest.first() {
        if line.indent != indent || is_sequence_item(line.content) {
            break;
        }
        let (key, raw_value) = split_key_value(line)?;
        rest = &rest[1..];
        if raw_value.is_empty() {
            match rest.first() {
                Some(next) if next.indent > indent => {
                    let (value, remaining) = parse_block(rest, next.indent)?;
                    entries.push((key, value));
                    rest = remaining;
                }
                _ => entries.push((key, Value::Null)),
            }
        } else {
            entries.push((key, parse_inline(raw_value, line.number)?));
        }
    }
    Ok((Value::Mapping(entries), rest))
}

fn is_flow(content: &str) -> bool {
    content.starts_with('{') || content.starts_with('[')
}

fn parse_inline(raw: &str, line: usize) -> Result<Value, ParseError> {
    if !is_flow(raw) {
        return Ok(parse_scalar(raw));
    }
    let mut flow = Flow {
        text: raw,
        src: raw.as_bytes(),
        pos: 0,
    };
    let fail = |message: String| ParseError { line, message };
    let value = flow.value().map_err(fail)?;
    flow.skip_ws();
    if flow.pos < flow.src.len() {
        return Err(ParseError {
            line,
            message: format!("unexpected '{}' after flow collection", &raw[flow.pos..]),
        });
    }
    Ok(value)
}

// Cursor over a single-line flow collection. `pos` only ever rests on an ASCII
// delimiter or the end of the text, so slicing `text` at it is always valid.
struct Flow<'a> {
    text: &'a str,
    src: &'a [u8],
    pos: usize,
}

impl<'a> Flow<'a> {
    fn skip_ws(&mut self) {
        while matches!(self.src.get(self.pos), Some(b' ') | Some(b'\t')) {
            self.pos += 1;
        }
    }

    fn value(&mut self) -> Result<Value, String> {
        self.skip_ws();
        match self.src.get(self.pos) {
            Some(b'{') => self.mapping(),
            Some(b'[') => self.sequence(),
            Some(b'"') | Some(b'\'') => Ok(Value::String(unquote(&self.quoted()?))),
            _ => Ok(parse_scalar(self.plain(false))),
        }
    }

    fn sequence(&mut self) -> Result<Value, String> {
        self.pos += 1;
        let mut items = Vec::new();
        loop {
            self.skip_ws();
            match self.src.get(self.pos) {
                Some(b']') => {
                    self.pos += 1;
                    return Ok(Value::Sequence(items));
                }
                None => return Err("unterminated flow sequence".to_string()),
                _ => {}
            }
            items.push(self.value()?);
            self.skip_ws();
            match self.src.get(self.pos) {
                Some(b',') => self.pos += 1,
                Some(b']') => {}
                _ => return Err("expected ',' or ']' in flow sequence".to_string()),
            }
        }
    }

    fn mapping(&mut self) -> Result<Value, String> {
        self.pos += 1;
        let mut entries = Vec::new();
        loop {
            self.skip_ws();
            match self.src.get(self.pos) {
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(Value::Mapping(entries));
                }
                None => return Err("unterminated flow mapping".to_string()),
                _ => {}
            }
            let key = match self.src.get(self.pos) {
                Some(b'"') | Some(b'\'') => unquote(&self.quoted()?),
                _ => self.plain(true).trim().to_string(),
            };
            self.skip_ws();
            let value = if self.src.get(self.pos) == Some(&b':') {
                self.pos += 1;
                self.skip_ws();
                match self.src.get(self.pos) {
                    Some(b',') | Some(b'}') => Value::Null,
                    _ => self.value()?,
                }
            } else {
                Value::Null
            };
            entries.push((key, value));
            self.skip_ws();
            match self.src.get(self.pos) {
                Some(b',') => self.pos += 1,
                Some(b'}') => {}
                _ => return Err("expected ',' or '}' in flow mapping".to_string()),
            }
        }
    }

    // Returns the quoted token including its quotes, so callers can unquote it
    // the same way block-style scalars are.
    fn quoted(&mut self) -> Result<String, String> {
        let src = self.src;
        let quote = src[self.pos];
        let start = self.pos;
        self.pos += 1;
        loop {
            match src.get(self.pos) {
                None => return Err("unterminated quoted string".to_string()),
                Some(b'\\') if quote == b'"' => self.pos += 2,
                Some(&c) if c == quote => {
                    if quote == b'\'' && src.get(self.pos + 1) == Some(&b'\'') {
                        self.pos += 2;
                    } else {
                        self.pos += 1;
                        return Ok(self.text[start..self.pos].to_string());
                    }
                }
                _ => self.pos += 1,
            }
        }
    }

    // A plain scalar ends at a flow delimiter. As a mapping key it also ends
    // at a colon that is followed by whitespace or a delimiter, so URLs and
    // times in values keep their colons.
    fn plain(&mut self, is_key: bool) -> &'a str {
        let src = self.src;
        let start = self.pos;
        while let Some(&b) = src.get(self.pos) {
            if matches!(b, b',' | b'}' | b']') {
                break;
            }
            if is_key && b == b':' {
                let next = src.get(self.pos + 1);
                if matches!(next, None | Some(b' ') | Some(b',') | Some(b'}')) {
                    break;
                }
            }
            self.pos += 1;
        }
        &self.text[start..self.pos]
    }
}

fn split_key_value<'a>(line: &Line<'a>) -> Result<(String, &'a str), ParseError> {
    let content = line.content;
    let colon_pos = find_key_separator(content).ok_or_else(|| ParseError {
        line: line.number,
        message: format!("expected 'key: value', got '{content}'"),
    })?;
    let key = unquote(content[..colon_pos].trim());
    let value = content[colon_pos + 1..].trim();
    Ok((key, value))
}

// A colon only separates key from value when it is followed by whitespace or
// end of line - otherwise "http://host" would be split on its first colon.
fn find_key_separator(content: &str) -> Option<usize> {
    let bytes = content.as_bytes();
    let mut in_single = false;
    let mut in_double = false;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'\'' if !in_double => in_single = !in_single,
            b'"' if !in_single => in_double = !in_double,
            b':' if !in_single && !in_double => {
                let at_boundary = i + 1 == bytes.len() || bytes[i + 1] == b' ';
                if at_boundary {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

fn unquote(raw: &str) -> String {
    if raw.len() >= 2 {
        if let Some(inner) = raw.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
            return inner.replace("\\\"", "\"").replace("\\\\", "\\");
        }
        if let Some(inner) = raw.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')) {
            return inner.replace("''", "'");
        }
    }
    raw.to_string()
}

pub fn parse_scalar(raw: &str) -> Value {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Value::Null;
    }
    let is_quoted = trimmed.len() >= 2
        && ((trimmed.starts_with('"') && trimmed.ends_with('"'))
            || (trimmed.starts_with('\'') && trimmed.ends_with('\'')));
    if is_quoted {
        return Value::String(unquote(trimmed));
    }
    match trimmed {
        "~" | "null" | "Null" | "NULL" => return Value::Null,
        "true" | "True" | "TRUE" => return Value::Bool(true),
        "false" | "False" | "FALSE" => return Value::Bool(false),
        _ => {}
    }
    if let Ok(n) = trimmed.parse::<f64>() {
        return Value::Number(n);
    }
    Value::String(trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalars() {
        assert_eq!(parse_scalar("true"), Value::Bool(true));
        assert_eq!(parse_scalar("FALSE"), Value::Bool(false));
        assert_eq!(parse_scalar("null"), Value::Null);
        assert_eq!(parse_scalar("~"), Value::Null);
        assert_eq!(parse_scalar("42"), Value::Number(42.0));
        assert_eq!(parse_scalar("3.5"), Value::Number(3.5));
        assert_eq!(parse_scalar("hello"), Value::String("hello".to_string()));
        assert_eq!(
            parse_scalar("\"quoted: value\""),
            Value::String("quoted: value".to_string())
        );
    }

    #[test]
    fn flat_mapping() {
        let value = parse("name: demo\nport: 8080\nenabled: true\n").unwrap();
        assert_eq!(
            value,
            Value::Mapping(vec![
                ("name".to_string(), Value::String("demo".to_string())),
                ("port".to_string(), Value::Number(8080.0)),
                ("enabled".to_string(), Value::Bool(true)),
            ])
        );
    }

    #[test]
    fn nested_mapping() {
        let value = parse("service:\n  name: demo\n  port: 8080\n").unwrap();
        assert_eq!(
            value,
            Value::Mapping(vec![(
                "service".to_string(),
                Value::Mapping(vec![
                    ("name".to_string(), Value::String("demo".to_string())),
                    ("port".to_string(), Value::Number(8080.0)),
                ])
            )])
        );
    }

    #[test]
    fn sequence_of_scalars() {
        let value = parse("hosts:\n  - a\n  - b\n").unwrap();
        assert_eq!(
            value,
            Value::Mapping(vec![(
                "hosts".to_string(),
                Value::Sequence(vec![
                    Value::String("a".to_string()),
                    Value::String("b".to_string()),
                ])
            )])
        );
    }

    #[test]
    fn sequence_of_mappings() {
        let value = parse("tags:\n  - name: env\n    value: prod\n").unwrap();
        assert_eq!(
            value,
            Value::Mapping(vec![(
                "tags".to_string(),
                Value::Sequence(vec![Value::Mapping(vec![
                    ("name".to_string(), Value::String("env".to_string())),
                    ("value".to_string(), Value::String("prod".to_string())),
                ])])
            )])
        );
    }

    #[test]
    fn comments_and_blank_lines_are_ignored() {
        let value = parse("# top comment\nname: demo # trailing\n\nport: 80\n").unwrap();
        assert_eq!(
            value,
            Value::Mapping(vec![
                ("name".to_string(), Value::String("demo".to_string())),
                ("port".to_string(), Value::Number(80.0)),
            ])
        );
    }

    #[test]
    fn flow_sequence_value() {
        let value = parse("ports: [80, 443, \"a, b\"]\n").unwrap();
        assert_eq!(
            value,
            Value::Mapping(vec![(
                "ports".to_string(),
                Value::Sequence(vec![
                    Value::Number(80.0),
                    Value::Number(443.0),
                    Value::String("a, b".to_string()),
                ])
            )])
        );
    }

    #[test]
    fn flow_mapping_value_with_nesting() {
        let value = parse("svc: {name: demo, url: http://h:1, tags: [a, b], extra: {}}\n").unwrap();
        assert_eq!(
            value,
            Value::Mapping(vec![(
                "svc".to_string(),
                Value::Mapping(vec![
                    ("name".to_string(), Value::String("demo".to_string())),
                    ("url".to_string(), Value::String("http://h:1".to_string())),
                    (
                        "tags".to_string(),
                        Value::Sequence(vec![
                            Value::String("a".to_string()),
                            Value::String("b".to_string()),
                        ])
                    ),
                    ("extra".to_string(), Value::Mapping(vec![])),
                ])
            )])
        );
    }

    #[test]
    fn flow_collection_as_sequence_item() {
        let value = parse("- {a: 1}\n- [x, y]\n").unwrap();
        assert_eq!(
            value,
            Value::Sequence(vec![
                Value::Mapping(vec![("a".to_string(), Value::Number(1.0))]),
                Value::Sequence(vec![
                    Value::String("x".to_string()),
                    Value::String("y".to_string()),
                ]),
            ])
        );
    }

    #[test]
    fn unterminated_flow_is_an_error() {
        let err = parse("a: 1\nb: [1, 2\n").unwrap_err();
        assert_eq!(err.line, 2);
        assert!(parse("a: {x: 1} tail\n").is_err());
    }

    #[test]
    fn missing_colon_is_an_error() {
        let err = parse("name demo\n").unwrap_err();
        assert_eq!(err.line, 1);
    }
}
