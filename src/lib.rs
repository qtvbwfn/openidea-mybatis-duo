//! MyBatis Duo: statement generation, the `@Param` quickfix and SQL-log
//! conversion. Navigation (gutter, Go to Definition, completion) is the
//! official `openidea.mybatis` plugin's job — this plugin adds what it lacks.

mod generate;
mod parse;
mod sqllog;

use openidea_plugin::providers::{CodeAction, Document, Providers};
use openidea_plugin::text::{offset, position};
use openidea_plugin::{Error, Invocation, NotificationLevel, Plugin, Range, TextEdit, host, register_plugin};

#[derive(Default)]
struct MybatisPlus;

impl Plugin for MybatisPlus {
    fn run_command(&mut self, id: &str, ctx: &Invocation, _arg: Option<&str>) -> Result<Option<String>, Error> {
        match id {
            "convert-sql-log" => convert_sql_log(ctx),
            "generate-statement" => generate_statement(ctx),
            "create-mapper-xml" => create_mapper_xml(ctx),
            other => Err(Error::NotFound(format!("command {other}"))),
        }
    }
}

/// `Copy MyBatis SQL` for the editor selection: the converter is pure, the
/// result goes to the clipboard.
fn convert_sql_log(ctx: &Invocation) -> Result<Option<String>, Error> {
    let editor = ctx.editor().ok_or_else(|| Error::Invalid("no file is open in the editor".into()))?;
    let doc = editor.document()?;
    let selections = editor.selections()?;
    let mut parts = Vec::new();
    for r in &selections {
        if let Some(text) = doc.text_in(*r) {
            parts.push(text.to_string());
        }
    }
    let Some(sql) = sqllog::convert(&parts.join("\n")) else {
        host::notify(
            NotificationLevel::Warning,
            "MyBatis SQL",
            "The selection has no MyBatis SQL log to convert",
        );
        return Ok(None);
    };
    host::copy_to_clipboard(&sql)?;
    host::notify(NotificationLevel::Info, "MyBatis SQL", "The converted SQL is on the clipboard");
    Ok(Some(sql))
}

/// `Create Mapper Statement`: the caret sits in a mapper method; the statement
/// goes into the matching mapper XML, or a new one is created after a
/// confirmation when none exists.
fn generate_statement(ctx: &Invocation) -> Result<Option<String>, Error> {
    let editor = ctx.editor().ok_or_else(|| Error::Invalid("no file is open in the editor".into()))?;
    if editor.language() != "java" {
        host::notify(NotificationLevel::Warning, "MyBatis Duo", "The caret is not in a Java file");
        return Ok(None);
    }
    let doc = editor.document()?;
    let selections = editor.selections()?;
    let cursor = selections.first().and_then(|r| offset(&doc.text, r.start)).unwrap_or(0);
    let Some(iface) = parse::parse_java(&doc.text) else {
        host::notify(NotificationLevel::Warning, "MyBatis Duo", "The file is not a mapper interface");
        return Ok(None);
    };
    let method = iface
        .methods
        .iter()
        .find(|m| cursor >= m.extent.0 && cursor < m.extent.1);
    let Some(method) = method else {
        host::notify(NotificationLevel::Warning, "MyBatis Duo", "Put the caret in a mapper method");
        return Ok(None);
    };
    let Some(kind) = method.kind else {
        host::notify(
            NotificationLevel::Warning,
            "MyBatis Duo",
            "This method carries SQL annotations or is a default method — it needs no XML statement",
        );
        return Ok(None);
    };
    let project = ctx.project().ok_or_else(|| Error::Invalid("no project is open".into()))?;
    // shortcut: mapper XMLs conventionally end with `Mapper.xml`; other names
    // need the file opened once to be found (a project-wide namespace scan can
    // come later).
    let candidates = project.find_files("**/*Mapper.xml", 200).unwrap_or_default();
    for path in candidates {
        let Ok(text) = project.read_file(&path) else { continue };
        let xml = parse::parse_xml(&text);
        if xml.namespace.as_deref() != Some(iface.fqn.as_str()) {
            continue;
        }
        if xml.statements.iter().any(|s| s == &method.name) {
            host::notify(
                NotificationLevel::Info,
                "MyBatis Duo",
                &format!("{}#{} already has an XML statement", iface.name, method.name),
            );
            return Ok(None);
        }
        let statement = generate::statement_xml(method, &iface);
        let (from, to, insert) = generate::append_statement_edit(&text, &statement);
        let updated = generate::apply_edits(&text, &[(from, to, insert)]);
        match project.write_file(&path, &updated) {
            Ok(()) => {
                host::notify(
                    NotificationLevel::Info,
                    "MyBatis Duo",
                    &format!("Created {} <{}> in {}", kind.as_str(), method.name, path),
                );
                return Ok(Some(path));
            }
            Err(Error::Invalid(_)) => {
                host::notify(
                    NotificationLevel::Warning,
                    "MyBatis Duo",
                    &format!("Save {} first: it has unsaved changes", path),
                );
                return Ok(None);
            }
            Err(e) => return Err(e),
        }
    }
    // No mapper XML for this interface yet: name the command that creates one.
    host::notify(
        NotificationLevel::Warning,
        "MyBatis Duo",
        &format!("No mapper XML for {} — use 'MyBatis Duo: Create Mapper XML' first", iface.fqn),
    );
    Ok(None)
}

/// `Create Mapper XML`: the whole file for the interface, at the Spring
/// convention path under `src/main/resources`.
fn create_mapper_xml(ctx: &Invocation) -> Result<Option<String>, Error> {
    let editor = ctx.editor().ok_or_else(|| Error::Invalid("no file is open in the editor".into()))?;
    if editor.language() != "java" {
        host::notify(NotificationLevel::Warning, "MyBatis Duo", "The caret is not in a Java file");
        return Ok(None);
    }
    let doc = editor.document()?;
    let Some(iface) = parse::parse_java(&doc.text) else {
        host::notify(NotificationLevel::Warning, "MyBatis Duo", "The file is not a mapper interface");
        return Ok(None);
    };
    let project = ctx.project().ok_or_else(|| Error::Invalid("no project is open".into()))?;
    let root = project.root()?;
    let rel = format!("src/main/resources/{}/{}.xml", iface.pkg.replace('.', "/"), iface.name);
    let target = format!("{}/{}", root.trim_end_matches('/'), rel);
    if project.exists(&target)? {
        host::notify(
            NotificationLevel::Info,
            "MyBatis Duo",
            &format!("{rel} already exists"),
        );
        return Ok(None);
    }
    project.write_file(&target, &generate::mapper_xml_content(&iface))?;
    host::notify(NotificationLevel::Info, "MyBatis Duo", &format!("Created {rel}"));
    Ok(Some(target))
}

impl Providers for MybatisPlus {
    /// `Add @Param annotations` as a quickfix on the mapper method under the
    /// caret; edits stay in the current file (annotations + the import).
    fn code_actions(&mut self, doc: &Document, selection: Range) -> Vec<CodeAction> {
        if doc.language != "java" {
            return Vec::new();
        }
        let Some(iface) = parse::parse_java(&doc.text) else { return Vec::new() };
        let Some(cursor) = offset(&doc.text, selection.start) else { return Vec::new() };
        let Some(method) = iface
            .methods
            .iter()
            .find(|m| cursor >= m.extent.0 && cursor < m.extent.1)
        else {
            return Vec::new();
        };
        let mut edits = generate::param_annotation_edits(&iface, Some(method));
        if edits.is_empty() {
            return Vec::new();
        }
        if let Some(import) = generate::import_edit(&doc.text) {
            edits.push(import);
        }
        let text_edits: Vec<TextEdit> = edits
            .iter()
            .map(|(from, to, insert)| TextEdit {
                range: Range::new(position(&doc.text, *from), position(&doc.text, *to)),
                text: insert.clone(),
            })
            .collect();
        vec![CodeAction {
            title: format!("Add @Param annotations to {}", method.name),
            kind: Some("quickfix".into()),
            preferred: true,
            edits: vec![(doc.path.clone(), text_edits)],
            data: None,
        }]
    }
}

register_plugin!(MybatisPlus, providers);

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    const MAPPER: &str = r#"
package com.example.mapper;
import java.util.List;

public interface UserMapper {
    List<User> findByName(String name, int size);
}
"#;

    #[test]
    fn the_param_quickfix_fires_inside_the_method() {
        let host = MockHost::install();
        host.open_editor("/p/src/UserMapper.java", "java", MAPPER, &[(5, 12, 5, 12)]);
        let doc = Document {
            path: "/p/src/UserMapper.java".into(),
            language: "java".into(),
            version: 1,
            text: MAPPER.into(),
        };
        let actions = MybatisPlus::default().code_actions(&doc, host.invocation().editor().unwrap().selections().unwrap()[0]);
        assert_eq!(actions.len(), 1, "{actions:?}");
        assert!(actions[0].title.contains("findByName"));
        assert_eq!(actions[0].edits[0].1.len(), 3, "two annotations + the import");
    }

    #[test]
    fn conversion_copies_to_the_clipboard() {
        let host = MockHost::install();
        let log = "==>  Preparing: select 1\n==> Parameters: \n";
        host.open_editor("/p/a.log", "log", log, &[(0, 0, 1, 0)]);
        let out = MybatisPlus::default()
            .run_command("convert-sql-log", &host.invocation(), None)
            .unwrap();
        assert_eq!(out.as_deref(), Some("select 1;"));
        assert_eq!(host.clipboard().as_deref(), Some("select 1;"));
    }
}