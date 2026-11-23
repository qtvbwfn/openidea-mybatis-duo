//! MyBatis code generation: statement XML from a mapper method, a whole
//! mapper.xml from an interface, `@Param` insertion plans and the import edit.
//! All pure; the callers write the file or hand the edits to the IDE.

use crate::parse::{JavaMethod, JavaParam, Kind, MapperInterface, resolve_type};

const IND: &str = "    "; // statements indent 4 spaces (MyBatisX convention)

/// A replacement: bytes `from..to` of the text become `insert`.
pub type Edit = (usize, usize, String);

const COLLECTION_PREFIXES: &[&str] = &[
    "java.util.List",
    "List",
    "java.util.Collection",
    "Collection",
    "java.util.Set",
    "Set",
    "java.util.Iterable",
    "Iterable",
    "java.util.ArrayList",
    "ArrayList",
    "java.util.LinkedList",
    "LinkedList",
    "java.util.HashSet",
    "HashSet",
    "java.util.LinkedHashSet",
    "LinkedHashSet",
    "java.util.TreeSet",
    "TreeSet",
    "java.util.Queue",
    "Queue",
    "java.util.Deque",
    "Deque",
];

/// `#{…}` references for the TODO comment: multi-params, `#{list}` for a single collection.
fn refs_hint(params: &[JavaParam]) -> String {
    match params.len() {
        0 => String::new(),
        1 => {
            let p = &params[0];
            let collection = COLLECTION_PREFIXES
                .iter()
                .any(|c| p.ty == *c || p.ty.starts_with(&format!("{c}<")))
                || p.ty.ends_with("[]");
            if collection {
                "; params: #{list}".to_string()
            } else {
                String::new()
            }
        }
        _ => format!(
            "; params: {}",
            params
                .iter()
                .map(|p| format!("#{{{}}}", p.binding))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

fn strip_generics(ty: &str) -> &str {
    match ty.find('<') {
        Some(i) => ty[..i].trim(),
        None => ty.trim(),
    }
}

/// `resultType` for a select: the element type, a count scalar, or `None` (TODO).
fn result_type_of(m: &JavaMethod, iface: &MapperInterface) -> Option<String> {
    if m.kind != Some(Kind::Select) {
        return None;
    }
    if let Some(simple) = crate::parse::result_element(&m.return_type) {
        return Some(resolve_type(&simple, iface));
    }
    let simple = strip_generics(&m.return_type);
    // Count scalars (`long count()`, `boolean exists()`): the type itself is the result.
    if ["boolean", "int", "long", "short", "byte"].contains(&simple) {
        return Some(simple.to_string());
    }
    if ["Boolean", "Integer", "Long", "Short", "Byte"].contains(&simple) {
        return Some(resolve_type(simple, iface));
    }
    None // Map or unresolved generics: leave for a resultMap
}

/// `parameterType` for the single parameter; primitives and arrays kept as-is.
fn parameter_type_of(p: &JavaParam, iface: &MapperInterface) -> String {
    let base = strip_generics(&p.ty);
    let (simple, array) = match base.strip_suffix("[]") {
        Some(s) => (s, true),
        None => (base, false),
    };
    let resolved = resolve_type(simple, iface);
    if array {
        format!("{resolved}[]")
    } else {
        resolved
    }
}

const DOTLESS_SCALARS: &[&str] = &[
    "boolean",
    "Boolean",
    "int",
    "Integer",
    "long",
    "Long",
    "short",
    "Short",
    "byte",
    "Byte",
    "float",
    "Float",
    "double",
    "Double",
    "String",
    "Date",
    "BigDecimal",
];

pub fn statement_xml(m: &JavaMethod, iface: &MapperInterface, kind: Kind) -> String {
    let mut attrs: Vec<String> = Vec::new();
    let rt = result_type_of(m, iface);
    if let Some(rt) = &rt {
        attrs.push(format!("resultType=\"{rt}\""));
    }
    let mut pt: Option<String> = None;
    if m.params.len() == 1 {
        let t = parameter_type_of(&m.params[0], iface);
        attrs.push(format!("parameterType=\"{t}\""));
        pt = Some(t);
    }
    let attr_text = if attrs.is_empty() {
        String::new()
    } else {
        format!(" {}", attrs.join(" "))
    };
    let body = format!(
        "<!-- TODO: write {}{} -->",
        kind.as_str().to_uppercase(),
        refs_hint(&m.params)
    );
    let mut lines = vec![
        format!("<{} id=\"{}\"{}>", kind.as_str(), m.name, attr_text),
        format!("{IND}{body}"),
    ];
    if kind == Kind::Insert {
        lines.push(format!(
            "{IND}<!-- TODO: add useGeneratedKeys=\"true\" keyProperty=\"id\" to return generated keys -->"
        ));
    }
    // Simple names without a known import are not valid MyBatis types (no
    // alias resolution): flag them instead of guessing.
    let unresolved = [&rt, &pt]
        .into_iter()
        .flatten()
        .any(|t| !t.contains('.') && !DOTLESS_SCALARS.contains(&t.as_str()));
    if unresolved {
        lines.push(format!(
            "{IND}<!-- TODO: use a fully qualified type (unresolved import) -->"
        ));
    }
    lines.push(format!("</{}>", kind.as_str()));
    lines
        .iter()
        .map(|l| format!("{IND}{l}"))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn mapper_xml_content(iface: &MapperInterface) -> String {
    let stmts: Vec<String> = iface
        .methods
        .iter()
        .filter(|m| m.kind.is_some() && !m.default_method)
        .map(|m| statement_xml(m, iface, m.kind.unwrap_or(Kind::Select)))
        .collect();
    let body = stmts.join("\n\n");
    let header = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" ?>\n",
        "<!DOCTYPE mapper PUBLIC \"-//mybatis.org//DTD Mapper 3.0//EN\" \"http://mybatis.org/dtd/mybatis-3-mapper.dtd\">\n"
    );
    let namespace = format!("<mapper namespace=\"{}\">", iface.fqn);
    if body.is_empty() {
        format!("{header}{namespace}\n\n</mapper>\n")
    } else {
        format!("{header}{namespace}\n\n{body}\n</mapper>\n")
    }
}

/// The whole mapper XML carrying just one statement: what a fresh mapper for
/// the method starts with (the interface's other methods generate on demand).
pub fn mapper_xml_with_statement(m: &JavaMethod, iface: &MapperInterface, kind: Kind) -> String {
    // Same valid header as `mapper_xml_content` (the DOCTYPE must close with `>`,
    // else the emitted XML fails to parse at the DOCTYPE line).
    let header = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" ?>\n",
        "<!DOCTYPE mapper PUBLIC \"-//mybatis.org//DTD Mapper 3.0//EN\" \"http://mybatis.org/dtd/mybatis-3-mapper.dtd\">\n"
    );
    format!(
        "{header}<mapper namespace=\"{}\">\n\n{}\n</mapper>\n",
        iface.fqn,
        statement_xml(m, iface, kind)
    )
}

/// Where a generated statement goes: before `</mapper>`, else the end of the file.
pub fn append_statement_edit(xml_text: &str, stmt_xml: &str) -> Edit {
    if let Some(close) = xml_text.rfind("</mapper>") {
        let prefix = if close > 0 && xml_text.as_bytes()[close - 1] == b'\n' {
            ""
        } else {
            "\n"
        };
        (close, close, format!("{stmt_xml}\n\n{prefix}"))
    } else {
        let prefix = if xml_text.ends_with('\n') { "" } else { "\n" };
        (
            xml_text.len(),
            xml_text.len(),
            format!("{prefix}{stmt_xml}\n"),
        )
    }
}

// ---------------------------------------------------------------------------
// @Param

/// `@Param` edits for `scope` (one method) or the whole interface: a written
/// `@Param` / `@Param()` without a value is filled with the parameter name,
/// and multi-parameter methods get `@Param("name") ` before every parameter
/// that has none (single-parameter methods need no annotation in MyBatis).
/// The single parameter when it is a collection/array: MyBatis binds it under
/// its default aliases (`list` / `collection` / `array`), so annotating it with
/// the Java name would rename the binding and break existing XML that references
/// those aliases (`BindingException: Parameter 'list' not found`). Such a
/// parameter is left untouched — the default-alias contract wins over the
/// annotation.
fn single_collection_param(m: &JavaMethod) -> Option<&JavaParam> {
    let [p] = &m.params[..] else { return None };
    let collection = COLLECTION_PREFIXES
        .iter()
        .any(|c| p.ty == *c || p.ty.starts_with(&format!("{c}<")))
        || p.ty.ends_with("[]");
    collection.then_some(p)
}

/// `@Param` edits for `scope` (one method) or the whole interface: a written
/// `@Param` / `@Param()` without a value is filled with the parameter name,
/// and multi-parameter methods get `@Param("name") ` before every parameter
/// that has none. A lone collection/array parameter is left untouched (see
/// [`single_collection_param`]); a lone scalar one still gets annotated.
pub fn param_annotation_edits(iface: &MapperInterface, scope: Option<&JavaMethod>) -> Vec<Edit> {
    let methods: &[JavaMethod] = match scope {
        Some(m) => std::slice::from_ref(m),
        None => &iface.methods,
    };
    let mut edits = Vec::new();
    for m in methods {
        // A lone collection/array parameter keeps MyBatis' default-alias binding;
        // annotating it would break XML that references `list`/`collection`/`array`.
        if single_collection_param(m).is_some() {
            continue;
        }
        for p in &m.params {
            if p.has_param {
                continue;
            }
            if let Some((from, to)) = p.empty_param_at {
                edits.push((from, to, format!("@Param(\"{}\")", p.name)));
            } else {
                edits.push((p.start, p.start, format!("@Param(\"{}\") ", p.name)));
            }
        }
    }
    edits
}

/// Inserts the `org.apache.ibatis.annotations.Param` import; `None` when present.
pub fn import_edit(text: &str) -> Option<Edit> {
    let mut last_import_end: Option<usize> = None;
    let mut has_param = false;
    let mut pos = 0usize;
    for line in text.split_inclusive('\n') {
        let t = line.trim_start();
        if let Some(rest) = t.strip_prefix("import ")
            && let Some(name) = rest
                .trim_end()
                .trim_end_matches(';')
                .trim()
                .rsplit('.')
                .next()
        {
            if name == "Param" {
                has_param = true;
            }
            if rest
                .trim_end()
                .trim_end_matches(';')
                .trim()
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '.')
            {
                last_import_end = Some(pos + line.trim_end_matches('\n').len());
            }
        }
        pos += line.len();
    }
    if has_param {
        return None;
    }
    if let Some(last) = last_import_end {
        let eol = if text[last..].starts_with("\r\n") {
            "\r\n"
        } else {
            "\n"
        };
        return Some((
            last,
            last,
            format!("{eol}import org.apache.ibatis.annotations.Param;"),
        ));
    }
    // After the package declaration, else at the top.
    for line in text.split_inclusive('\n') {
        let t = line.trim_start();
        if t.starts_with("package ")
            && let Some(end) = t.find(';')
        {
            let at = (line.as_ptr() as usize - text.as_ptr() as usize)
                + (line.len() - line.trim_start().len())
                + end
                + 1;
            return Some((
                at,
                at,
                "\nimport org.apache.ibatis.annotations.Param;".to_string(),
            ));
        }
    }
    Some((
        0,
        0,
        "import org.apache.ibatis.annotations.Param;\n\n".to_string(),
    ))
}

/// Applies non-overlapping edits (descending order).
pub fn apply_edits(text: &str, edits: &[Edit]) -> String {
    let mut sorted: Vec<&Edit> = edits.iter().collect();
    sorted.sort_by(|a, b| b.0.cmp(&a.0));
    let mut out = text.to_string();
    for (from, to, insert) in sorted {
        out.replace_range(*from..*to, insert);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse_java;

    const MAPPER: &str = r#"
package com.example.mapper;
import java.util.List;
import com.example.User;

public interface UserMapper {
    List<User> findByStatus(String status, @Param() int size);

    void saveUser(String name, Integer age);

    int countAll(@Param("type") String type);
}
"#;

    #[test]
    fn statement_xml_carries_result_and_parameter_types() {
        let iface = parse_java(MAPPER).unwrap();
        let m = &iface.methods[0]; // List<User> findByStatus(String, int)
        let xml = statement_xml(m, &iface, m.kind.unwrap_or(Kind::Select));
        assert!(
            xml.contains("    <select id=\"findByStatus\" resultType=\"com.example.User\">"),
            "{xml}"
        );
        assert!(xml.contains("params: #{status}, #{size}"), "{xml}");
        let save = statement_xml(&iface.methods[1], &iface, Kind::Insert); // void saveUser(String, Integer)
        assert!(save.contains("<insert id=\"saveUser\">"), "{save}");
        assert!(save.contains("useGeneratedKeys"), "{save}");
    }

    #[test]
    fn param_edits_fill_empty_and_multi_param_methods() {
        let iface = parse_java(MAPPER).unwrap();
        let m = &iface.methods[0];
        let edits = param_annotation_edits(&iface, Some(m));
        assert_eq!(edits.len(), 2, "{edits:?}");
        assert!(edits.iter().any(|(_, _, ins)| ins == "@Param(\"size\")"));
        assert!(edits.iter().any(|(_, _, ins)| ins == "@Param(\"status\") "));
        // Single-parameter methods with @Param stay untouched.
        assert!(param_annotation_edits(&iface, Some(&iface.methods[2])).is_empty());
    }

    #[test]
    fn single_collection_parameter_keeps_its_default_alias() {
        // A single collection/array parameter is bound by MyBatis under the
        // default aliases (`list`/`collection`/`array`); annotating it with the
        // Java name would break XML that references those aliases (P2 from the
        // registry review, MyBatis 3.5.19 BindingException). So no edit is
        // produced for it, and a lone scalar parameter is still annotated.
        let text = r#"
package com.example.mapper;
import java.util.List;
import com.example.User;

public interface M {
    int saveAll(List<String> users);

    User findById(long id);
}
"#;
        let iface = parse_java(text).unwrap();
        assert!(
            param_annotation_edits(&iface, Some(&iface.methods[0])).is_empty(),
            "single collection parameter must not be annotated"
        );
        let scalar = param_annotation_edits(&iface, Some(&iface.methods[1]));
        assert_eq!(scalar.len(), 1, "{scalar:?}");
        assert!(scalar[0].2.contains("@Param(\"id\")"), "{scalar:?}");
    }

    #[test]
    fn mapper_xml_with_statement_has_closed_doctype_and_parses() {
        let iface = parse_java(MAPPER).unwrap();
        let m = &iface.methods[0];
        let xml = mapper_xml_with_statement(m, &iface, m.kind.unwrap_or(Kind::Select));
        // The DOCTYPE must close with `>` (the bug wrote it without the terminator).
        assert!(xml.contains("mybatis-3-mapper.dtd\">\n"), "{xml}");
        // The emitted XML parses and exposes its namespace and statement id.
        let parsed = crate::nav::parse_xml(&xml);
        assert_eq!(
            parsed.namespace.as_ref().map(|n| n.name.as_str()),
            Some("com.example.mapper.UserMapper")
        );
        assert!(
            parsed.statements.iter().any(|s| s.name == "findByStatus"),
            "{xml}"
        );
    }

    #[test]
    fn import_edit_adds_the_annotation_import() {
        let edit = import_edit("package a.b;\npublic interface X {}").expect("edit");
        let out = apply_edits("package a.b;\npublic interface X {}", &[edit]);
        assert!(
            out.contains("package a.b;\nimport org.apache.ibatis.annotations.Param;"),
            "{out}"
        );
        assert!(import_edit("import org.apache.ibatis.annotations.Param;\n").is_none());
    }

    #[test]
    fn append_places_the_statement_before_the_close_tag() {
        let xml = "<mapper namespace=\"a\">\n\n</mapper>";
        let (from, to, insert) = append_statement_edit(xml, "    <select id=\"x\"></select>");
        let out = apply_edits(xml, &[(from, to, insert)]);
        assert!(
            out.ends_with("<select id=\"x\"></select>\n\n</mapper>"),
            "{out}"
        );
    }
}
