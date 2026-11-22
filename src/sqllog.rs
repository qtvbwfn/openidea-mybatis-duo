//! MyBatis console log → executable SQL (BatsLog-style): every `==>  Preparing:`
//! statement gets its `?` placeholders filled from the following
//! `==> Parameters: v1(T1), v2(T2)` line — the logged type decides (numbers and
//! booleans print raw, strings, characters, dates and timestamps are quoted and
//! escaped, and a local date-time's ISO `T` becomes a space, so
//! `00123(String)` keeps its leading zeros and `2024-05-01T10:20:30` is a
//! timestamp literal), and only untyped values fall back to guessing. A `null`
//! value is SQL `NULL` whatever type the log names beside it. Several pairs in
//! the selection become several statements, joined with `;\n\n`, each ending in
//! joined with `;\n\n`, each ending in `;`. The SQL text itself is kept exactly
//! as MyBatis logged it (no formatting).
//!
//! shortcut: parameters must sit on one line (MyBatis prints them so), and a
//! value literally containing `), ` cannot be round-tripped.

/// Types whose logged value is already a SQL literal.
const NUMERIC_TYPES: &[&str] = &[
    "Long",
    "Integer",
    "Short",
    "Byte",
    "Double",
    "Float",
    "BigDecimal",
    "BigInteger",
    "Number",
    "AtomicInteger",
    "AtomicLong",
    "long",
    "int",
    "short",
    "byte",
    "double",
    "float",
];
/// Types whose logged value is already a SQL boolean literal.
const BOOLEAN_TYPES: &[&str] = &["Boolean", "boolean", "AtomicBoolean"];
/// Date/time types: their text needs quotes, but `null` is SQL NULL — unlike
/// a text type, a temporal value has no `"null"` text form to quote.
const TEMPORAL_TYPES: &[&str] = &[
    "Date",
    "Time",
    "Timestamp",
    "LocalDate",
    "LocalTime",
    "LocalDateTime",
    "OffsetDateTime",
    "OffsetTime",
    "ZonedDateTime",
    "Instant",
    "Year",
    "YearMonth",
    "MonthDay",
    "Duration",
    "Period",
];

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
            out.push(ParsedParam {
                value: bare.join(", "),
                ty: None,
            });
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
            out.push(ParsedParam {
                value: joined.join(", "),
                ty: Some(ty.to_string()),
            });
        } else if segment == "null" {
            flush(&mut out, &mut bare);
            out.push(ParsedParam {
                value: "null".to_string(),
                ty: None,
            });
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
    !int.is_empty()
        && int.chars().all(|c| c.is_ascii_digit())
        && parts.next().is_none()
        && frac.is_none_or(|f| !f.is_empty() && f.chars().all(|c| c.is_ascii_digit()))
}

fn format_value(p: &ParsedParam) -> String {
    // `null` is SQL NULL whatever the type says: MyBatis prints a null with no
    // value text, and a String whose text reads `null` is indistinguishable in
    // the log — the converter picks the SQL meaning.
    if p.value == "null" {
        return "NULL".to_string();
    }
    let Some(ty) = &p.ty else {
        // Untyped: MyBatis logs a bare `null` (handled above); any other bare
        // value keeps the guess (only hand-edited logs reach this path).
        return if is_number(&p.value) || p.value == "true" || p.value == "false" {
            p.value.clone()
        } else {
            quote(&p.value)
        };
    };
    // The log names the type: honour it instead of guessing from the text, or
    // `00123(String)` loses its zeros and `true(String)` becomes a bool.
    let ty = ty.rsplit('.').next().unwrap_or(ty); // java.lang.String → String
    if NUMERIC_TYPES.contains(&ty) {
        // Numbers print raw; a value that isn't a number is quoted rather than
        // emitted as broken SQL.
        return if is_number(&p.value) {
            p.value.clone()
        } else {
            quote(&p.value)
        };
    }
    if BOOLEAN_TYPES.contains(&ty) {
        return if p.value == "true" || p.value == "false" {
            p.value.clone()
        } else {
            quote(&p.value)
        };
    }
    if TEMPORAL_TYPES.contains(&ty) {
        // `LocalDateTime` and friends print with an ISO `T`; MySQL and Oracle
        // only parse the space form. Offsets and `Z` stay as logged — a
        // literal with a zone needs a dialect-specific cast.
        return quote(&local_datetime(&p.value).unwrap_or_else(|| p.value.clone()));
    }
    // String, Character and unknown types: quote and escape.
    quote(&p.value)
}

/// `2024-05-01T10:20:30.123` → `2024-05-01 10:20:30.123`, and nothing for
/// values that carry an offset or `Z` (or are not a local date-time at all).
fn local_datetime(value: &str) -> Option<String> {
    let (date, time) = value.split_once('T')?;
    let date_ok = date.len() == 10
        && date.bytes().enumerate().all(|(i, b)| {
            if i == 4 || i == 7 {
                b == b'-'
            } else {
                b.is_ascii_digit()
            }
        });
    let time_ok = !time.is_empty()
        && time
            .bytes()
            .all(|b| b.is_ascii_digit() || b == b':' || b == b'.');
    (date_ok && time_ok).then(|| format!("{date} {time}"))
}

fn quote(v: &str) -> String {
    format!("'{}'", v.replace('\'', "''"))
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
    Some(
        statements
            .iter()
            .map(|s| format!("{};", s.trim_end()))
            .collect::<Vec<_>>()
            .join("\n\n"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairs_statements_and_substitutes_parameters() {
        let log = "==>  Preparing: select * from t where id = ? and name = ?\n\
                   ==> Parameters: 1(Long), admin(String)\n";
        assert_eq!(
            convert(log).as_deref(),
            Some("select * from t where id = 1 and name = 'admin';")
        );
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
    fn honours_explicit_string_type() {
        // Issue #2: the logged `(String)` must win over the numeric/boolean shape
        // of the value (`00123` is not 123, `true` is not a bool).
        let log = "==>  Preparing: select * from users where code = ? and flag = ? and extra = ?\n\
                   ==> Parameters: 00123(String), true(String), null(String)\n";
        assert_eq!(
            convert(log).as_deref(),
            Some("select * from users where code = '00123' and flag = 'true' and extra = NULL;")
        );
    }

    #[test]
    fn qualified_type_names_are_normalised() {
        let log = "==>  Preparing: select ?, ?\n\
                   ==> Parameters: 7(java.lang.Long), 00123(java.lang.String)\n";
        assert_eq!(convert(log).as_deref(), Some("select 7, '00123';"));
    }

    #[test]
    fn every_type_category_keeps_its_sql_shape() {
        let log = "==>  Preparing: select ?, ?, ?, ?, ?, ?, ?, ?\n\
                   ==> Parameters: 42(Long), 3.5(BigDecimal), 007(Integer), true(Boolean), \
                   2024-05-01(Date), 2024-05-01 10:20:30.0(Timestamp), 10:20:30(LocalTime), a(Character)\n";
        assert_eq!(
            convert(log).as_deref(),
            Some(
                "select 42, 3.5, 007, true, '2024-05-01', '2024-05-01 10:20:30.0', '10:20:30', 'a';"
            )
        );
    }

    #[test]
    fn iso_local_date_times_use_the_space_form() {
        // LocalDateTime prints `2024-05-01T10:20:30`; MySQL and Oracle parse
        // the space form, so the `T` becomes one. A value with a zone is kept
        // exactly as logged (writing it as a literal depends on the dialect).
        let log = "==>  Preparing: select ?\n\
                   ==> Parameters: 2024-05-01T10:20:30(LocalDateTime)\n\
                   ==>  Preparing: select ?\n\
                   ==> Parameters: 2024-05-01T10:20:30.123(LocalDateTime)\n\
                   ==>  Preparing: select ?\n\
                   ==> Parameters: 2024-05-01T02:20:30Z(Instant)\n\
                   ==>  Preparing: select ?\n\
                   ==> Parameters: 2024-05-01T10:20:30+08:00(OffsetDateTime)\n";
        assert_eq!(
            convert(log).as_deref(),
            Some(
                "select '2024-05-01 10:20:30';\n\nselect '2024-05-01 10:20:30.123';\n\n\
                 select '2024-05-01T02:20:30Z';\n\nselect '2024-05-01T10:20:30+08:00';"
            )
        );
    }

    #[test]
    fn null_is_sql_null_whatever_the_type() {
        // The log prints a null with no value text, so `null` means SQL NULL
        // even beside a text type (a String reading "null" is not told apart).
        let log = "==>  Preparing: select ?, ?, ?, ?, ?\n\
                   ==> Parameters: null(String), null(Long), null(Timestamp), null(Boolean), null\n";
        assert_eq!(
            convert(log).as_deref(),
            Some("select NULL, NULL, NULL, NULL, NULL;")
        );
    }

    #[test]
    fn a_value_out_of_shape_for_its_type_is_quoted() {
        // Hand-edited logs can put text where the type says number: never emit
        // it raw as broken SQL.
        let log = "==>  Preparing: select ?, ?\n\
                   ==> Parameters: not-a-number(Long), yes(Boolean)\n";
        assert_eq!(
            convert(log).as_deref(),
            Some("select 'not-a-number', 'yes';")
        );
    }

    #[test]
    fn untyped_values_fall_back_to_guessing() {
        // Only a bare `null` is untyped in a real log; the guess stays for
        // hand-edited input.
        let log = "==>  Preparing: select ?\n\
                   ==> Parameters: 42\n\
                   ==>  Preparing: select ?\n\
                   ==> Parameters: true\n\
                   ==>  Preparing: select ?\n\
                   ==> Parameters: abc\n";
        assert_eq!(
            convert(log).as_deref(),
            Some("select 42;\n\nselect true;\n\nselect 'abc';")
        );
    }

    #[test]
    fn keeps_short_placeholders_and_joins_several_statements() {
        let log = "==>  Preparing: select ? and ?\n\
                   ==> Parameters: 1(Long)\n\
                   ==>  Preparing: delete from t\n";
        assert_eq!(
            convert(log).as_deref(),
            Some("select 1 and ?;\n\ndelete from t;")
        );
    }

    #[test]
    fn returns_none_without_a_preparing_line() {
        assert_eq!(convert("just some text\n==> Parameters: 1(Long)"), None);
    }
}
