//! MyBatis console log → executable SQL (BatsLog-style): every `==>  Preparing:`
//! statement gets its `?` placeholders filled from the following
//! `==> Parameters: v1(T1), v2(T2)` line — numbers raw, strings quoted and
//! escaped, null → NULL. Several pairs in the selection become several
//! statements, joined with `;\n\n`, each ending in `;`. The SQL text itself is
//! kept exactly as MyBatis logged it (no formatting).
//!
//! shortcut: parameters must sit on one line (MyBatis prints them so), and a
//! value literally containing `), ` cannot be round-tripped.

const NUMERIC_TYPES: &[&str] =
    &["Long", "Integer", "Short", "Byte", "Double", "Float", "BigDecimal", "BigInteger"];

struct ParsedParam {
    value: String,
    ty: Option<String>,
}

/// Splits `1(Long), admin(String), null` into parameter pairs. Values may
/// contain commas (`a, b(String)`) because only a `(Type)` ending (or a bare
/// `null`) closes a parameter: bare segments are re-joined with the next
/// typed one.
fn parse_params(s: &str) -> Vec<ParsedParam> {
    let mut out = Vec::new();
    let mut bare: Vec<String> = Vec::new();
    let flush = |out: &mut Vec<ParsedParam>, bare: &mut Vec<String>| {
        if !bare.is_empty() {
            out.push(ParsedParam { value: bare.join(", "), ty: None });
            bare.clear();
        }
    };
    for raw in s.split(',') {
        let segment = raw.trim();
        if segment.is_empty() {
            continue;
        }
        if let Some(close) = segment.strip_suffix(')')
            && let Some(open) = close.rfind('(')
        {
            let value = &close[..open];
            let ty = &close[open + 1..];
            let mut joined: Vec<String> = bare.drain(..).filter(|x| !x.is_empty()).collect();
            if !value.is_empty() {
                joined.push(value.to_string());
            }
            out.push(ParsedParam { value: joined.join(", "), ty: Some(ty.to_string()) });
        } else if segment == "null" {
            flush(&mut out, &mut bare);
            out.push(ParsedParam { value: "null".to_string(), ty: None });
        } else {
            bare.push(segment.to_string());
        }
    }
    flush(&mut out, &mut bare);
    out
}

fn is_number(v: &str) -> bool {
    let body = v.strip_prefix('-').unwrap_or(v);
    let mut parts = body.split('.');
    let int = parts.next().unwrap_or("");
    let frac = parts.next();
    !int.is_empty() && int.chars().all(|c| c.is_ascii_digit()) && parts.next().is_none()
        && frac.is_none_or(|f| !f.is_empty() && f.chars().all(|c| c.is_ascii_digit()))
}

fn format_value(p: &ParsedParam) -> String {
    if p.value == "null" {
        return "NULL".to_string();
    }
    if let Some(ty) = &p.ty {
        if NUMERIC_TYPES.contains(&ty.as_str()) || ty == "Boolean" {
            return p.value.clone();
        }
    }
    // Typeless values (only null normally) fall back to a numeric / boolean guess.
    if is_number(&p.value) || p.value == "true" || p.value == "false" {
        return p.value.clone();
    }
    format!("'{}'", p.value.replace('\'', "''"))
}

fn fill_params(sql: &str, params: &[ParsedParam]) -> String {
    let mut i = 0;
    let mut out = String::with_capacity(sql.len());
    for c in sql.chars() {
        if c == '?' {
            // Short params keep their `?`; extra params are ignored.
            match params.get(i) {
                Some(p) => {
                    out.push_str(&format_value(p));
                    i += 1;
                }
                None => out.push('?'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// The `Preparing:` payload of a line, if any.
fn preparing(line: &str) -> Option<&str> {
    let rest = line.split_once("==>")?.1;
    let rest = rest.trim_start();
    Some(rest.strip_prefix("Preparing:")?.trim_start())
}

/// The `Parameters:` payload of a line, if any.
fn parameters(line: &str) -> Option<&str> {
    let rest = line.split_once("==>")?.1;
    let rest = rest.trim_start();
    Some(rest.strip_prefix("Parameters:")?.trim_start())
}

/// The executable SQL of the selection, or `None` without any `Preparing` line.
pub fn convert(text: &str) -> Option<String> {
    let mut statements: Vec<String> = Vec::new();
    let mut pending: Option<String> = None;
    for line in text.lines() {
        if let Some(sql) = preparing(line) {
            // A previous statement without parameters keeps its placeholders.
            if let Some(p) = pending.take() {
                statements.push(p);
            }
            pending = Some(sql.to_string());
            continue;
        }
        if let Some(values) = parameters(line)
            && let Some(sql) = pending.take()
        {
            statements.push(fill_params(&sql, &parse_params(values)));
        }
    }
    if let Some(p) = pending {
        statements.push(p);
    }
    if statements.is_empty() {
        return None;
    }
    Some(statements.iter().map(|s| format!("{};", s.trim_end())).collect::<Vec<_>>().join("\n\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairs_statements_and_substitutes_parameters() {
        let log = "==>  Preparing: select * from t where id = ? and name = ?\n\
                   ==> Parameters: 1(Long), admin(String)\n";
        assert_eq!(convert(log).as_deref(), Some("select * from t where id = 1 and name = 'admin';"));
    }

    #[test]
    fn quotes_escapes_and_nulls() {
        let log = "==>  Preparing: select ?\n\
                   ==> Parameters: O'Brien(String)\n";
        assert_eq!(convert(log).as_deref(), Some("select 'O''Brien';"));
        let log = "==>  Preparing: select ?, ?\n\
                   ==> Parameters: null, 3.5(Double)\n";
        assert_eq!(convert(log).as_deref(), Some("select NULL, 3.5;"));
    }

    #[test]
    fn keeps_short_placeholders_and_joins_several_statements() {
        let log = "==>  Preparing: select ? and ?\n\
                   ==> Parameters: 1(Long)\n\
                   ==>  Preparing: delete from t\n";
        assert_eq!(convert(log).as_deref(), Some("select 1 and ?;\n\ndelete from t;"));
    }

    #[test]
    fn returns_none_without_a_preparing_line() {
        assert_eq!(convert("just some text\n==> Parameters: 1(Long)"), None);
    }
}