//! Java mapper interfaces and mapper XML, parsed from text: package, imports,
//! the type and its method signatures (params with types and `@Param` state),
//! and the XML namespace + statement ids.
//!
//! Adapted from OpenIDEA's official MyBatis Navigator plugin
//! (`plugins/official/mybatis`, Apache-2.0) and extended with the return type
//! and parameter details that generation and the `@Param` quickfix need.
//!
//! shortcut: no full Java parser — pathologically formatted signatures may be
//! missed, and methods inherited from a parent interface are not resolved.

use std::collections::BTreeMap;

/// Statement kind, inferred from the method.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Select,
    Insert,
    Update,
    Delete,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Select => "select",
            Kind::Insert => "insert",
            Kind::Update => "update",
            Kind::Delete => "delete",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct JavaParam {
    /// Type text with generics, e.g. `List<String>`; `final` and annotations stripped.
    pub ty: String,
    /// Declared parameter name.
    pub name: String,
    /// XML binding name: the `@Param` value when annotated, else `name`.
    pub binding: String,
    /// The parameter already carries `@Param("…")`.
    pub has_param: bool,
    /// A `@Param` / `@Param()` without a value, ready to fill with the name.
    pub empty_param_at: Option<(usize, usize)>,
    /// Byte offset of the parameter's first non-space character.
    pub start: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct JavaMethod {
    pub name: String,
    /// Statement kind for XML generation; `None` for annotation-SQL methods.
    pub kind: Option<Kind>,
    /// `@Select` / `@Insert` / `@Update` / `@Delete` on the method, when present.
    pub sql_annotation: bool,
    /// Raw return-type text, e.g. `List<User>` or `void`.
    pub return_type: String,
    pub params: Vec<JavaParam>,
    /// `default` / `static` methods take no XML statement.
    pub default_method: bool,
    /// Signature span: byte offsets, from the first char to after `;` / before `{`.
    pub span: (usize, usize),
    /// Display extent: from the previous member's end (annotations included).
    pub extent: (usize, usize),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct JavaImports {
    /// Simple name → FQN (`import a.b.User;`).
    pub exact: BTreeMap<String, String>,
    /// Packages from `import a.b.*;`.
    pub wildcards: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MapperInterface {
    pub name: String,
    pub pkg: String,
    pub fqn: String,
    pub methods: Vec<JavaMethod>,
    pub imports: JavaImports,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MapperXml {
    pub namespace: Option<String>,
    /// Statement ids (`<select|insert|update|delete id="…">`).
    pub statements: Vec<String>,
}

// ---------------------------------------------------------------------------
// Masking

/// Comments and string literals replaced by spaces (newlines kept), so byte
/// offsets in the mask index the original text 1:1.
pub fn mask(text: &str) -> Vec<u8> {
    let b = text.as_bytes();
    let mut out = b.to_vec();
    let blank = |out: &mut [u8], from: usize, to: usize| {
        for byte in &mut out[from..to] {
            if *byte != b'\n' && *byte != b'\r' {
                *byte = b' ';
            }
        }
    };
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == b'"' || c == b'\'' {
            let mut j = i + 1;
            while j < b.len() {
                if b[j] == b'\\' {
                    j += 2;
                } else if b[j] == c {
                    j += 1;
                    break;
                } else {
                    j += 1;
                }
            }
            let j = j.min(b.len());
            blank(&mut out, i, j);
            i = j;
        } else if c == b'/' && b.get(i + 1) == Some(&b'/') {
            let mut j = i + 2;
            while j < b.len() && b[j] != b'\n' {
                j += 1;
            }
            blank(&mut out, i, j);
            i = j;
        } else if c == b'/' && b.get(i + 1) == Some(&b'*') {
            let mut j = i + 2;
            while j + 1 < b.len() && !(b[j] == b'*' && b[j + 1] == b'/') {
                j += 1;
            }
            let j = (j + 2).min(b.len());
            blank(&mut out, i, j);
            i = j;
        } else {
            i += 1;
        }
    }
    out
}

fn is_ident(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

fn find_bytes(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

// ---------------------------------------------------------------------------
// Type declaration

struct TypeDecl {
    name: String,
    body_from: usize,
}

fn find_type_decl(masked: &[u8]) -> Option<TypeDecl> {
    let kw = b"interface ";
    let mut from = 0;
    while let Some(rel) = find_bytes(&masked[from..], kw) {
        let at = from + rel;
        // Skip `@interface` annotation declarations.
        if at > 0 && masked[at - 1] == b'@' {
            from = at + kw.len();
            continue;
        }
        if at > 0 && is_ident(masked[at - 1]) {
            from = at + kw.len();
            continue;
        }
        let name_start = at + kw.len();
        let name: String = masked[name_start..]
            .iter()
            .take_while(|b| is_ident(**b))
            .map(|b| *b as char)
            .collect();
        if name.is_empty() {
            return None;
        }
        // Body open: the first `{` at zero depth of `()[]{}`.
        let (mut paren, mut bracket, mut brace) = (0i32, 0i32, 0i32);
        let mut body_from = None;
        for k in name_start + name.len()..masked.len() {
            match masked[k] {
                b'(' => paren += 1,
                b')' => paren = (paren - 1).max(0),
                b'[' => bracket += 1,
                b']' => bracket = (bracket - 1).max(0),
                b'{' => {
                    if paren == 0 && bracket == 0 && brace == 0 {
                        body_from = Some(k);
                        break;
                    }
                    brace += 1;
                }
                b'}' => brace = (brace - 1).max(0),
                b';' => return None,
                _ => {}
            }
        }
        return body_from.map(|body_from| TypeDecl { name, body_from });
    }
    None
}

// ---------------------------------------------------------------------------
// Imports

fn collect_imports(masked: &[u8]) -> JavaImports {
    let mut exact = BTreeMap::new();
    let mut wildcards = Vec::new();
    for line in masked.split(|b| *b == b'\n') {
        let t = trim_start(line);
        let Some(rest) = t.strip_prefix(b"import ") else { continue };
        let rest = trim_start(rest);
        if rest.starts_with(b"static ") {
            continue;
        }
        let Some(end) = rest.iter().position(|b| *b == b';') else { continue };
        let fqn = String::from_utf8_lossy(&rest[..end]).trim().to_string();
        if let Some(pkg) = fqn.strip_suffix(".*") {
            wildcards.push(pkg.to_string());
        } else if let Some(simple) = fqn.rsplit('.').next() {
            exact.insert(simple.to_string(), fqn);
        }
    }
    JavaImports { exact, wildcards }
}

fn trim_start(b: &[u8]) -> &[u8] {
    let mut i = 0;
    while i < b.len() && (b[i] as char).is_whitespace() {
        i += 1;
    }
    &b[i..]
}

// ---------------------------------------------------------------------------
// Annotations

/// `@name(…)` removed from a declaration prefix (`@Foo(a, (b))` included).
fn strip_annotations(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'@' {
            let mut j = i + 1;
            while j < b.len() && (is_ident(b[j]) || b[j] == b'.') {
                j += 1;
            }
            // Balanced annotation arguments.
            while j < b.len() && (b[j] as char).is_whitespace() {
                j += 1;
            }
            if j < b.len() && b[j] == b'(' {
                let mut depth = 0;
                while j < b.len() {
                    match b[j] {
                        b'(' | b'<' | b'[' | b'{' => depth += 1,
                        b')' | b'>' | b']' | b'}' => {
                            depth -= 1;
                            if depth == 0 {
                                j += 1;
                                break;
                            }
                        }
                        _ => {}
                    }
                    j += 1;
                }
            }
            out.push(' ');
            i = j;
        } else {
            out.push(b[i] as char);
            i += 1;
        }
    }
    out
}

const MODIFIERS: &[&str] = &[
    "public", "protected", "private", "static", "final", "abstract", "default", "native", "synchronized",
    "strictfp", "transient", "volatile", "new", "throws",
];

/// The return type between the annotation-stripped prefix and the method name.
fn return_type_of(prefix: &str) -> String {
    let mut p: String = strip_annotations(prefix)
        .split_whitespace()
        .filter(|w| !MODIFIERS.contains(&w.trim_end_matches(';')))
        .collect::<Vec<_>>()
        .join(" ");
    // Leading method type parameters (`<T> T select(T t)`).
    while p.starts_with('<') {
        let mut depth = 0;
        let mut close = None;
        for (k, c) in p.char_indices() {
            match c {
                '<' => depth += 1,
                '>' => {
                    depth -= 1;
                    if depth == 0 {
                        close = Some(k);
                        break;
                    }
                }
                _ => {}
            }
        }
        match close {
            Some(k) => p = p[k + 1..].trim().to_string(),
            None => break,
        }
    }
    p.trim().to_string()
}

// ---------------------------------------------------------------------------
// Methods

struct MethodSpan {
    name: String,
    name_at: usize,
    paren_open: usize,
    paren_close: usize,
    sig_start: usize,
    sig_end: usize,
}

fn match_paren(masked: &[u8], open_at: usize) -> Option<usize> {
    let mut depth = 0i32;
    for k in open_at..masked.len() {
        match masked[k] {
            b'(' | b'<' | b'[' | b'{' => depth += 1,
            b')' | b'>' | b']' | b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(k);
                }
            }
            _ => {}
        }
    }
    None
}

fn find_method_spans(masked: &[u8], decl: &TypeDecl, body_to: usize) -> Vec<MethodSpan> {
    let mut out = Vec::new();
    let mut i = decl.body_from;
    while i < masked.len() && i < body_to {
        if !is_ident(masked[i]) || (i > 0 && is_ident(masked[i - 1])) {
            i += 1;
            continue;
        }
        let name_at = i;
        let mut j = i;
        while j < masked.len() && is_ident(masked[j]) {
            j += 1;
        }
        let name_end = j;
        // Next non-space char must be `(`.
        let mut k = name_end;
        while k < masked.len() && (masked[k] as char).is_whitespace() {
            k += 1;
        }
        if k >= masked.len() || masked[k] != b'(' {
            i = name_end;
            continue;
        }
        let name = String::from_utf8_lossy(&masked[name_at..name_end]).to_string();
        // The char before the name: whitespace, `>` or a boundary.
        let before = name_at.checked_sub(1).and_then(|x| masked.get(x)).copied();
        let ok_before = match before {
            None => true,
            Some(b) => (b as char).is_whitespace() || b == b'>' || b == b'{' || b == b';',
        };
        if !ok_before {
            i = name_end;
            continue;
        }
        // Walk back to the previous declaration boundary.
        let mut depth = 0i32;
        let mut boundary = decl.body_from;
        for q in (decl.body_from + 1..name_at).rev() {
            match masked[q] {
                b'}' => {
                    if depth == 0 {
                        boundary = q;
                        break;
                    }
                    depth += 1;
                }
                b'{' => {
                    if depth == 0 {
                        boundary = q;
                        break;
                    }
                    depth -= 1;
                }
                b';' if depth == 0 => {
                    boundary = q;
                    break;
                }
                _ => {}
            }
        }
        let segment = String::from_utf8_lossy(&masked[boundary + 1..name_at]).to_string();
        let cleaned = strip_annotations(&segment);
        let cleaned_trim = cleaned.trim();
        let has_word = cleaned_trim.chars().any(|c| c.is_alphanumeric() || c == '$');
        let bad = cleaned_trim.chars().any(|c| !(c.is_alphanumeric() || " _$<>[].?,@".contains(c)));
        if !has_word || bad || cleaned_trim.ends_with('.') {
            i = name_end;
            continue;
        }
        let rt = return_type_of(&cleaned);
        if rt.is_empty() {
            i = name_end;
            continue;
        }
        let paren_open = k;
        let Some(paren_close) = match_paren(masked, paren_open) else {
            i = name_end;
            continue;
        };
        // Signature end: after `)` comes `;`, `{`, or `throws …`.
        let mut e = paren_close + 1;
        let mut sig_end = None;
        while e < masked.len() {
            match masked[e] {
                b';' => {
                    sig_end = Some(e + 1);
                    break;
                }
                b'{' => {
                    sig_end = Some(e);
                    break;
                }
                c if (c as char).is_whitespace() => e += 1,
                _ => break,
            }
        }
        let Some(sig_end) = sig_end else {
            i = name_end;
            continue;
        };
        out.push(MethodSpan { name, name_at, paren_open, paren_close, sig_start: boundary + 1, sig_end });
        i = sig_end;
    }
    out
}

// ---------------------------------------------------------------------------
// Parameters

fn split_params(masked: &[u8], from: usize, to: usize) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start = from;
    for k in from..to.min(masked.len()) {
        match masked[k] {
            b'(' | b'<' | b'[' | b'{' => depth += 1,
            b')' | b'>' | b']' | b'}' => depth -= 1,
            b',' if depth == 0 => {
                out.push((start, k));
                start = k + 1;
            }
            _ => {}
        }
    }
    if start < to {
        out.push((start, to));
    }
    out
}

/// `@Param("name")` (qualified or not) on a parameter segment: its value span.
fn param_value(_text: &str, seg_start: usize, seg: &str) -> Option<(String, (usize, usize))> {
    let b = seg.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'@' {
            let mut j = i + 1;
            while j < b.len() && (is_ident(b[j]) || b[j] == b'.') {
                j += 1;
            }
            let ann = &seg[i + 1..j];
            if ann.rsplit('.').next() == Some("Param") {
                while j < b.len() && (b[j] as char).is_whitespace() {
                    j += 1;
                }
                if b.get(j) == Some(&b'(') {
                    // Find the matching `)` and a quoted value inside.
                    let mut depth = 0i32;
                    let mut end = None;
                    for k in j..b.len() {
                        match b[k] {
                            b'(' => depth += 1,
                            b')' => {
                                depth -= 1;
                                if depth == 0 {
                                    end = Some(k);
                                    break;
                                }
                            }
                            _ => {}
                        }
                    }
                    if let Some(end) = end {
                        let inner = &seg[j + 1..end];
                        let inner_trim = inner.trim();
                        if inner_trim.starts_with('"') && inner_trim.ends_with('"') && inner_trim.len() >= 2 {
                            let value = inner_trim[1..inner_trim.len() - 1].replace("\\\"", "\"");
                            return Some((value, (seg_start + i, seg_start + end + 1)));
                        }
                        // Empty `@Param()`: the fill point.
                        return Some((String::new(), (seg_start + i, seg_start + end + 1)));
                    }
                }
                // Bare `@Param` without parens: the fill point is the annotation alone.
                return Some((String::new(), (seg_start + i, seg_start + j)));
            }
        }
        i += 1;
    }
    None
}

fn parse_param(text: &str, from: usize, to: usize) -> Option<JavaParam> {
    let raw = &text[from..to.min(text.len())];
    let seg_start = from + (raw.len() - raw.trim_start().len());
    let mut seg = raw.trim().to_string();
    if seg.is_empty() {
        return None;
    }
    let annotated = param_value(text, seg_start, &seg);
    let (binding, empty_param_at, has_param) = match &annotated {
        Some((value, at)) if !value.is_empty() => (value.clone(), None, true),
        Some((_, at)) => (String::new(), Some(*at), false),
        None => (String::new(), None, false),
    };
    seg = strip_annotations(&seg)
        .split_whitespace()
        .filter(|w| *w != "final")
        .collect::<Vec<_>>()
        .join(" ");
    if seg.is_empty() {
        return None;
    }
    // Array brackets after the name (`String args[]`) belong to the type.
    let mut array_suffix = String::new();
    let mut seg = seg.trim_end().to_string();
    if seg.ends_with(']') {
        let cut = seg.rfind(|c: char| c != ' ' && c != '[' && c != ']').map(|i| i + 1).unwrap_or(0);
        let suffix = &seg[cut..];
        if suffix.chars().all(|c| c == '[' || c == ']' || c == ' ') {
            array_suffix = suffix.chars().filter(|c| *c == '[' || *c == ']').collect();
            seg = seg[..cut].trim_end().to_string();
        }
    }
    // Last identifier is the name.
    let name_start = seg
        .char_indices()
        .rev()
        .take_while(|(_, c)| c.is_alphanumeric() || *c == '_' || *c == '$')
        .last()
        .map(|(i, _)| i)?;
    let name = seg[name_start..].to_string();
    let mut ty = seg[..name_start].trim().to_string();
    if ty.is_empty() {
        return None;
    }
    // Varargs: `String... ids` → `String`.
    if let Some(stripped) = ty.strip_suffix("...") {
        ty = stripped.trim_end().to_string();
    }
    ty = format!("{ty}{array_suffix}");
    Some(JavaParam {
        ty,
        binding: if binding.is_empty() { name.clone() } else { binding },
        name,
        has_param,
        empty_param_at,
        start: seg_start,
    })
}

// ---------------------------------------------------------------------------
// Kinds and types

fn has_prefix(name: &str, prefixes: &[&str]) -> bool {
    prefixes.iter().any(|p| name.starts_with(p))
}

/// Statement kind by the return type, refined by the name prefix where the
/// type alone is ambiguous (`void` and the int/long/boolean counts).
pub fn infer_kind(name: &str, return_type: &str, sql_annotation: bool) -> Option<Kind> {
    if sql_annotation {
        return None;
    }
    let lower = name.to_lowercase();
    let simple = return_type.split('<').next().unwrap_or(return_type).trim();
    let count_scalars = [
        "boolean", "Boolean", "int", "Integer", "long", "Long", "short", "Short", "byte", "Byte",
    ];
    if simple == "void" {
        if has_prefix(&lower, &["insert", "save", "add", "create", "persist"]) {
            return Some(Kind::Insert);
        }
        if has_prefix(&lower, &["delete", "remove", "drop", "clear"]) {
            return Some(Kind::Delete);
        }
        return Some(Kind::Update);
    }
    if count_scalars.contains(&simple) {
        if has_prefix(&lower, &["delete", "remove", "drop", "clear"]) {
            return Some(Kind::Delete);
        }
        if has_prefix(&lower, &["insert", "save", "add", "create", "persist"]) {
            return Some(Kind::Insert);
        }
        if has_prefix(&lower, &["update", "modify", "edit", "change", "merge", "refresh"]) {
            return Some(Kind::Update);
        }
        if has_prefix(&lower, &["select", "find", "get", "query", "list", "search", "load", "count", "exists", "is", "has"])
        {
            return Some(Kind::Select);
        }
        return Some(Kind::Update);
    }
    Some(Kind::Select)
}

/// Element type for `resultType` (`User` for `List<User>`), `None` when unknown.
pub fn result_element(return_type: &str) -> Option<String> {
    let rt = return_type.trim();
    let collection_re = |prefixes: &[&str]| -> Option<String> {
        for p in prefixes {
            if let Some(rest) = rt.strip_prefix(p)
                && let Some(inner) = rest.trim_end().strip_suffix('>')
            {
                let inner = inner.trim();
                if !inner.is_empty() && inner.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$' || c == '.') {
                    return Some(inner.to_string());
                }
            }
        }
        None
    };
    let collections = [
        "List", "Collection", "Set", "Iterable", "ArrayList", "LinkedList", "HashSet", "LinkedHashSet", "TreeSet",
        "Queue", "Deque",
    ];
    let mut cands: Vec<String> = Vec::new();
    for c in collections {
        cands.push(format!("{c}<"));
        cands.push(format!("java.util.{c}<"));
    }
    cands.push("Optional<".into());
    cands.push("java.util.Optional<".into());
    let prefixed: Vec<&str> = cands.iter().map(String::as_str).collect();
    if let Some(e) = collection_re(&prefixed) {
        return Some(e);
    }
    if let Some(base) = rt.strip_suffix("[]") {
        let base = base.trim();
        if !base.is_empty() && base.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$' || c == '.') {
            return Some(base.to_string());
        }
    }
    if rt.starts_with("Map") || rt.starts_with("java.util.Map") {
        return None;
    }
    let void_or_count = [
        "void", "boolean", "Boolean", "int", "Integer", "long", "Long", "short", "Short", "byte", "Byte", "float",
        "Float", "double", "Double",
    ];
    if void_or_count.contains(&rt) {
        return None;
    }
    Some(rt.split('<').next().unwrap_or(rt).trim().to_string())
}

const JAVA_LANG: &[&str] = &[
    "String", "Object", "Boolean", "Byte", "Short", "Integer", "Long", "Float", "Double", "Character", "Void",
    "Number", "Math", "Enum", "Class", "Iterable", "Comparable",
];

/// Resolves a simple name against imports, `java.lang`, then the same package;
/// else the simple name.
pub fn resolve_type(simple: &str, iface: &MapperInterface) -> String {
    if simple.is_empty() {
        return String::new();
    }
    if simple.contains('.') {
        return simple.to_string();
    }
    if let Some(fqn) = iface.imports.exact.get(simple) {
        return fqn.clone();
    }
    if JAVA_LANG.contains(&simple) {
        return format!("java.lang.{simple}");
    }
    if !iface.imports.wildcards.is_empty() {
        return simple.to_string();
    }
    if !iface.pkg.is_empty() {
        return format!("{}.{simple}", iface.pkg);
    }
    simple.to_string()
}

// ---------------------------------------------------------------------------
// Entry points

pub fn parse_java(text: &str) -> Option<MapperInterface> {
    let masked = mask(text);
    let decl = find_type_decl(&masked)?;
    let pkg = find_bytes(&masked, b"package ")
        .and_then(|at| {
            let start = at + 8;
            let end = masked[start..].iter().position(|b| *b == b';')? + start;
            Some(String::from_utf8_lossy(&masked[start..end]).trim().to_string())
        })
        .unwrap_or_default();
    let fqn = if pkg.is_empty() { decl.name.clone() } else { format!("{}.{}", pkg, decl.name) };
    // Body close: brace depth back to zero.
    let mut depth = 0i32;
    let mut body_to = masked.len();
    for k in decl.body_from..masked.len() {
        match masked[k] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    body_to = k + 1;
                    break;
                }
            }
            _ => {}
        }
    }
    let mut methods = Vec::new();
    for span in find_method_spans(&masked, &decl, body_to) {
        let masked_sig = String::from_utf8_lossy(&masked[span.sig_start..span.sig_end]).to_string();
        let sql_annotation = sql_annotation_at(&masked_sig).is_some();
        let prefix = String::from_utf8_lossy(&masked[span.sig_start..span.name_at]).to_string();
        let return_type = return_type_of(&prefix);
        let default_method = masked_sig.split_whitespace().any(|w| w == "default" || w == "static");
        let mut params = Vec::new();
        for (from, to) in split_params(&masked, span.paren_open + 1, span.paren_close) {
            if let Some(p) = parse_param(text, from, to) {
                params.push(p);
            }
        }
        let prev_end = methods.last().map_or(decl.body_from, |m: &JavaMethod| m.span.1);
        methods.push(JavaMethod {
            name: span.name.clone(),
            kind: infer_kind(&span.name, &return_type, sql_annotation),
            sql_annotation,
            return_type,
            params,
            default_method,
            span: (span.sig_start, span.sig_end),
            extent: (prev_end, span.sig_end),
        });
    }
    Some(MapperInterface { name: decl.name, pkg, fqn, methods, imports: collect_imports(&masked) })
}

/// The SQL annotation name (`Select`, …) on a signature, if any.
fn sql_annotation_at(sig: &str) -> Option<String> {
    let b = sig.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'@' {
            let mut j = i + 1;
            while j < b.len() && (is_ident(b[j]) || b[j] == b'.') {
                j += 1;
            }
            let name = sig[i + 1..j].rsplit('.').next().unwrap_or("");
            if ["Select", "Insert", "Update", "Delete"].contains(&name) {
                return Some(name.to_string());
            }
            i = j;
        } else {
            i += 1;
        }
    }
    None
}

pub fn parse_xml(text: &str) -> MapperXml {
    let masked = mask(text);
    let namespace = find_bytes(&masked, b"namespace=").and_then(|at| {
        let start = at + 10;
        // The quote itself is masked (it opens a string literal): read it from the text.
        let quote = *text.as_bytes().get(start)?;
        if quote != b'"' && quote != b'\'' {
            return None;
        }
        let end = text[start + 1..].bytes().position(|b| b == quote)? + start + 1;
        Some(text[start + 1..end].to_string())
    });
    let mut statements = Vec::new();
    for tag in ["select", "insert", "update", "delete"] {
        let open = format!("<{tag} ");
        let mut from = 0;
        let masked_str = String::from_utf8_lossy(&masked).to_string();
        while let Some(rel) = masked_str[from..].find(&open) {
            let at = from + rel + open.len();
            if let Some(id) = id_attribute(text, at) {
                statements.push(id);
            }
            from = at;
        }
    }
    MapperXml { namespace, statements }
}

fn id_attribute(text: &str, from: usize) -> Option<String> {
    let rest = &text[from..];
    let at = rest.find("id=")?;
    // The attribute must start the attribute list (whitespace-separated).
    if rest[..at].chars().any(|c| c == '>') {
        return None;
    }
    let start = from + at + 3;
    let quote = text.as_bytes().get(start).copied()?;
    if quote != b'"' && quote != b'\'' {
        return None;
    }
    let end = text[start + 1..].bytes().position(|b| b == quote)? + start + 1;
    Some(text[start + 1..end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAPPER: &str = r#"
package com.example.mapper;
import java.util.List;
import com.example.User;
import org.apache.ibatis.annotations.Param;

public interface UserMapper {
    User getById(@Param("id") Long id);

    List<User> findByStatus(String status, @Param() int size);

    int deleteById(long id);

    void saveUser(String name, Integer age);

    @Select("select 1")
    int ping();

    default void helper() {}
}
"#;

    #[test]
    fn parses_methods_with_kinds_and_params() {
        let iface = parse_java(MAPPER).unwrap();
        assert_eq!(iface.fqn, "com.example.mapper.UserMapper");
        let names: Vec<&str> = iface.methods.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, ["getById", "findByStatus", "deleteById", "saveUser", "ping", "helper"]);
        assert_eq!(iface.methods[0].kind, Some(Kind::Select));
        assert_eq!(iface.methods[2].kind, Some(Kind::Delete));
        assert_eq!(iface.methods[3].kind, Some(Kind::Insert));
        assert_eq!(iface.methods[4].kind, None, "annotation SQL takes no XML statement");
        assert!(iface.methods[5].default_method, "default methods take no statement (filtered at generation)");
        let p = &iface.methods[0].params[0];
        assert_eq!((p.name.as_str(), p.binding.as_str(), p.has_param), ("id", "id", true));
        let p = &iface.methods[1].params[1];
        assert!(!p.has_param);
        assert!(p.empty_param_at.is_some(), "empty @Param() is a fill point");
    }

    #[test]
    fn resolves_and_unwraps_types() {
        let iface = parse_java(MAPPER).unwrap();
        assert_eq!(result_element("List<User>").unwrap(), "User");
        assert_eq!(result_element("void"), None);
        assert_eq!(result_element("Map<String,User>"), None);
        assert_eq!(resolve_type("String", &iface), "java.lang.String");
        assert_eq!(resolve_type("User", &iface), "com.example.User");
    }

    #[test]
    fn parses_mapper_xml() {
        let xml = r#"<mapper namespace="com.example.mapper.UserMapper">
    <select id="getById">select 1</select>
    <insert id="saveUser">insert</insert>
</mapper>"#;
        let parsed = parse_xml(xml);
        assert_eq!(parsed.namespace.as_deref(), Some("com.example.mapper.UserMapper"));
        assert_eq!(parsed.statements, ["getById", "saveUser"]);
    }
}