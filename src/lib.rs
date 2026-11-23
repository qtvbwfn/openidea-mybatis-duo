//! MyBatis Duo: mapper navigation both ways (gutter arrows, Go to Definition,
//! statement/parameter completion — ported from the official `openidea.mybatis`
//! plugin 0.4.1, see `nav.rs`), statement generation, the `@Param` quickfix and
//! SQL-log conversion. One plugin for the whole set: disable the official
//! Navigator to avoid double gutter arrows.

mod generate;
mod i18n;
mod nav;
mod parse;
mod sqllog;

use i18n::{t, t_with};
use openidea_plugin::indexing::DocumentIndexer;
use openidea_plugin::providers::{CodeAction, CompletionItem, Document, Providers};
use openidea_plugin::text::{offset, position};
use openidea_plugin::{Diagnostic, Severity};
use openidea_plugin::{
    Error, Invocation, NotificationAction, NotificationLevel, PickItem, Plugin, Position, Range,
    TextEdit, host, register_plugin,
};

#[derive(Default)]
struct MybatisPlus {
    /// The ported official navigator (namespace→XML cache included).
    nav: nav::Navigator,
}

impl Plugin for MybatisPlus {
    fn run_command(
        &mut self,
        id: &str,
        ctx: &Invocation,
        arg: Option<&str>,
    ) -> Result<Option<String>, Error> {
        match id {
            "convert-sql-log" => convert_sql_log(ctx, arg),
            "generate-statement" => generate_statement(&mut self.nav, ctx),
            "create-mapper-xml" => create_mapper_xml(ctx),
            "goto-mapper-xml" => goto_mapper_xml(&mut self.nav, ctx, arg),
            other => Err(Error::NotFound(format!("command {other}"))),
        }
    }
}

/// `Go to Mapper XML`: the caret on a mapper method opens its XML statement (or
/// `@Param`, result map, `<include>` target); an XML statement id opens the
/// method. Several targets are offered in a picker. This is the direct jump the
/// host's ⌘B no longer gives on a declaration (it shows usages there).
fn goto_mapper_xml(
    nav: &mut nav::Navigator,
    ctx: &Invocation,
    arg: Option<&str>,
) -> Result<Option<String>, Error> {
    // A resolved jump: "path|line|column" from the notification actions (`|`
    // keeps Windows drive letters whole). The command context carries the
    // project, so this opens where the bare resolve could not.
    if let Some(arg) = arg {
        let mut parts = arg.splitn(3, '|');
        if let (Some(path), Some(line), Some(col)) = (parts.next(), parts.next(), parts.next())
            && let (Ok(line), Ok(col)) = (line.parse::<u32>(), col.parse::<u32>())
        {
            host::open_location(path, line, col)?;
            return Ok(Some(path.to_owned()));
        }
    }
    let editor = ctx
        .editor()
        .ok_or_else(|| Error::Invalid(t("no file is open in the editor")))?;
    let text = editor.document()?;
    let doc = Document {
        path: editor.path(),
        language: editor.language(),
        version: text.version,
        text: text.text,
    };
    let selections = editor.selections()?;
    let Some(selection) = selections.first() else {
        return Ok(None);
    };
    let Some(cursor) = offset(&doc.text, selection.start) else {
        return Ok(None);
    };
    let targets = nav.definition(&doc, position(&doc.text, cursor));
    if targets.is_empty() {
        host::notify(
            NotificationLevel::Warning,
            "MyBatis Duo",
            &t("Put the caret on a mapper method or an XML statement id"),
        );
        return Ok(None);
    }
    let index = if targets.len() == 1 {
        0
    } else {
        let items: Vec<PickItem> = targets
            .iter()
            .enumerate()
            .map(|(i, target)| {
                let file = target
                    .path
                    .rsplit(['/', '\\'])
                    .next()
                    .unwrap_or(&target.path)
                    .to_owned();
                PickItem {
                    key: i.to_string(),
                    label: format!("{file}:{}", target.range.start.line + 1),
                    description: Some(target.path.clone()),
                }
            })
            .collect();
        match host::pick(&t("Go to Mapper"), &items)? {
            Some(key) => key.parse().unwrap_or(0),
            None => return Ok(None),
        }
    };
    let target = &targets[index];
    host::open_location(
        &target.path,
        target.range.start.line,
        target.range.start.character,
    )?;
    Ok(Some(target.path.clone()))
}

/// The statement kind to generate: the method name's prefix when it tells
/// (`insert`/`save`/…, `update`/`edit`/…, `del`/`delete`/`remove`/…), else the
/// user's pick. `None`: the user cancelled the choice.
fn statement_kind(method: &parse::JavaMethod) -> Result<Option<parse::Kind>, Error> {
    if let Some(kind) =
        parse::infer_kind_explicit(&method.name, &method.return_type, method.sql_annotation)
    {
        return Ok(Some(kind));
    }
    let items = [
        PickItem {
            key: "select".into(),
            label: "SELECT".into(),
            description: None,
        },
        PickItem {
            key: "insert".into(),
            label: "INSERT".into(),
            description: None,
        },
        PickItem {
            key: "update".into(),
            label: "UPDATE".into(),
            description: None,
        },
        PickItem {
            key: "delete".into(),
            label: "DELETE".into(),
            description: None,
        },
    ];
    Ok(
        host::pick(&t("Which statement kind does this method need?"), &items)?.and_then(|key| {
            match key.as_str() {
                "insert" => Some(parse::Kind::Insert),
                "update" => Some(parse::Kind::Update),
                "delete" => Some(parse::Kind::Delete),
                _ => Some(parse::Kind::Select),
            }
        }),
    )
}

/// `Copy MyBatis SQL`: the converter is pure, the result goes to the clipboard.
/// A passed `arg` (the host's Run console hands over its selection) is what gets
/// converted; without one the editor's selection is used.
fn convert_sql_log(ctx: &Invocation, arg: Option<&str>) -> Result<Option<String>, Error> {
    let text = match arg {
        Some(arg) => arg.to_owned(),
        None => {
            let editor = ctx
                .editor()
                .ok_or_else(|| Error::Invalid(t("no file is open in the editor")))?;
            let doc = editor.document()?;
            let selections = editor.selections()?;
            let mut parts = Vec::new();
            for r in &selections {
                if let Some(text) = doc.text_in(*r) {
                    parts.push(text.to_string());
                }
            }
            parts.join("\n")
        }
    };
    let Some(sql) = sqllog::convert(&text) else {
        host::notify(
            NotificationLevel::Warning,
            "MyBatis SQL",
            &t("The selection has no MyBatis SQL log to convert"),
        );
        return Ok(None);
    };
    host::copy_to_clipboard(&sql)?;
    host::notify(
        NotificationLevel::Info,
        "MyBatis SQL",
        &t("The converted SQL is on the clipboard"),
    );
    Ok(Some(sql))
}

/// `Create Mapper Statement`: the caret sits in a mapper method; the statement
/// goes into the matching mapper XML, or a new one is created after a
/// confirmation when none exists.
fn generate_statement(nav: &mut nav::Navigator, ctx: &Invocation) -> Result<Option<String>, Error> {
    let editor = ctx
        .editor()
        .ok_or_else(|| Error::Invalid(t("no file is open in the editor")))?;
    if editor.language() != "java" {
        host::notify(
            NotificationLevel::Warning,
            "MyBatis Duo",
            &t("The caret is not in a Java file"),
        );
        return Ok(None);
    }
    let doc = editor.document()?;
    let selections = editor.selections()?;
    let cursor = selections
        .first()
        .and_then(|r| offset(&doc.text, r.start))
        .unwrap_or(0);
    let Some(iface) = parse::parse_java(&doc.text) else {
        host::notify(
            NotificationLevel::Warning,
            "MyBatis Duo",
            &t("The file is not a mapper interface"),
        );
        return Ok(None);
    };
    let method = iface
        .methods
        .iter()
        .find(|m| cursor >= m.extent.0 && cursor < m.extent.1);
    let Some(method) = method else {
        host::notify(
            NotificationLevel::Warning,
            "MyBatis Duo",
            &t("Put the caret in a mapper method"),
        );
        return Ok(None);
    };
    let Some(&inferred) = method.kind.as_ref() else {
        host::notify(
            NotificationLevel::Warning,
            "MyBatis Duo",
            &t(
                "This method carries SQL annotations or is a default method — it needs no XML statement",
            ),
        );
        return Ok(None);
    };
    let Some(kind) = statement_kind(method)? else {
        return Ok(None);
    };
    let project = ctx
        .project()
        .ok_or_else(|| Error::Invalid(t("no project is open")))?;
    // The navigation index answers faster and never misses (the resolve path
    // uses the same lookup); a straight glob caps out on real projects.
    let Some((xml_path, xml)) = nav.xml_for(&project, &iface.fqn) else {
        host::notify(
            NotificationLevel::Warning,
            "MyBatis Duo",
            &t_with(
                "No mapper XML for {fqn} — use 'MyBatis Duo: Create Mapper XML' first",
                &[("fqn", &iface.fqn)],
            ),
        );
        return Ok(None);
    };
    if xml.statements.iter().any(|s| s.name == method.name) {
        host::notify(
            NotificationLevel::Info,
            "MyBatis Duo",
            &t_with(
                "{name}#{method} already has an XML statement",
                &[("name", &iface.name), ("method", &method.name)],
            ),
        );
        return Ok(None);
    }
    let text = project.read_file(&xml_path)?;
    let statement = generate::statement_xml(method, &iface, kind);
    let (from, to, insert) = generate::append_statement_edit(&text, &statement);
    let updated = generate::apply_edits(&text, &[(from, to, insert)]);
    match project.write_file(&xml_path, &updated) {
        Ok(()) => {
            host::notify(
                NotificationLevel::Info,
                "MyBatis Duo",
                &t_with(
                    "Created {kind} <{method}> in {path}",
                    &[
                        ("kind", kind.as_str()),
                        ("method", &method.name),
                        ("path", &xml_path),
                    ],
                ),
            );
            return Ok(Some(xml_path.clone()));
        }
        Err(Error::Invalid(_)) => {
            host::notify(
                NotificationLevel::Warning,
                "MyBatis Duo",
                &t_with(
                    "Save {path} first: it has unsaved changes",
                    &[("path", &xml_path)],
                ),
            );
            return Ok(None);
        }
        Err(e) => return Err(e),
    }
}

/// `Create Mapper XML`: the whole file for the interface, at the Spring
/// convention path under `src/main/resources`.
fn create_mapper_xml(ctx: &Invocation) -> Result<Option<String>, Error> {
    let editor = ctx
        .editor()
        .ok_or_else(|| Error::Invalid(t("no file is open in the editor")))?;
    if editor.language() != "java" {
        host::notify(
            NotificationLevel::Warning,
            "MyBatis Duo",
            &t("The caret is not in a Java file"),
        );
        return Ok(None);
    }
    let doc = editor.document()?;
    let Some(iface) = parse::parse_java(&doc.text) else {
        host::notify(
            NotificationLevel::Warning,
            "MyBatis Duo",
            &t("The file is not a mapper interface"),
        );
        return Ok(None);
    };
    let project = ctx
        .project()
        .ok_or_else(|| Error::Invalid(t("no project is open")))?;
    let root = project.root()?;
    let rel = format!(
        "src/main/resources/{}/{}.xml",
        iface.pkg.replace('.', "/"),
        iface.name
    );
    let target = format!("{}/{}", root.trim_end_matches('/'), rel);
    if project.exists(&target)? {
        host::notify(
            NotificationLevel::Info,
            "MyBatis Duo",
            &t_with("{rel} already exists", &[("rel", &rel)]),
        );
        return Ok(None);
    }
    project.write_file(&target, &generate::mapper_xml_content(&iface))?;
    host::notify(
        NotificationLevel::Info,
        "MyBatis Duo",
        &t_with("Created {rel}", &[("rel", &rel)]),
    );
    Ok(Some(target))
}

impl DocumentIndexer for MybatisPlus {
    /// Background indexing: the host feeds documents in batches (the 5 s
    /// command budget) and answers gutter arrows and Go to Definition from
    /// the graph, so navigation never runs inside the 500 ms edit budget.
    fn index(
        &mut self,
        doc: &openidea_plugin::indexing::Document,
    ) -> openidea_plugin::indexing::Edges {
        let edges = self.nav.index(doc);
        self.publish_param_diagnostics(doc);
        edges
    }
}

impl MybatisPlus {
    /// @Param diagnostics for a mapper interface's multi-parameter methods: a
    /// mapper's extra parameters need `@Param` for the `#{…}` bindings, and the
    /// resolve/generate actions add them. Publishing replaces the file's set,
    /// so fixed parameters clear their warnings. Interfaces that are not
    /// mappers — by the name's own suffix, or unless the index has seen their
    /// mapper XML — stay clean, so ordinary interfaces get no warnings.
    fn publish_param_diagnostics(&mut self, doc: &openidea_plugin::indexing::Document) {
        if !doc.path.ends_with(".java") {
            return;
        }
        let Some(iface) = parse::parse_java(&doc.text) else {
            let _ = host::publish_diagnostics(&doc.path, &[]);
            return;
        };
        let is_mapper = self.nav.mapper_files.contains_key(&iface.fqn)
            || iface.fqn.rsplit('.').next().is_some_and(|simple| {
                simple.ends_with("Mapper")
                    || simple.ends_with("Dao")
                    || simple.ends_with("Repository")
            });
        let mut items: Vec<Diagnostic> = Vec::new();
        if is_mapper {
            for method in &iface.methods {
                if method.default_method {
                    continue;
                }
                for (from, _to, _) in generate::param_annotation_edits(&iface, Some(method)) {
                    items.push(Diagnostic {
                        range: Range::new(position(&doc.text, from), position(&doc.text, from)),
                        severity: Severity::Warning,
                        message: t_with(
                            "Parameter of {name} has no @Param — generate it with MyBatis Duo",
                            &[("name", &method.name)],
                        ),
                        code: Some("mybatis-param".into()),
                        source: None,
                        unnecessary: false,
                        deprecated: false,
                        related: Vec::new(),
                    });
                }
            }
        }
        let _ = host::publish_diagnostics(&doc.path, &items);
    }
}

impl Providers for MybatisPlus {
    /// Statement ids, `<sql>` refids, `#{…}` parameters, result-map and
    /// annotation-SQL parameter completion. (Gutter arrows and Go to
    /// Definition are answered from the index graph — see `DocumentIndexer`.)
    fn completion(
        &mut self,
        doc: &Document,
        at: Position,
        trigger: Option<&str>,
    ) -> Vec<CompletionItem> {
        self.nav.completion(doc, at, trigger)
    }

    /// The MyBatis actions appear only for what the mapper lacks: the XML exists
    /// → no Create Mapper XML; the statement is there → no Generate Statement;
    /// every parameter is annotated → no Add @Param. The states come from the
    /// plugin's own navigation index (`indexed_mapper`), which the gutters keep
    /// warm, so no filesystem access is needed on the 50 ms edit path — until
    /// the index has seen the project, the actions stay offered and the resolve
    /// confirms.
    fn code_actions(&mut self, doc: &Document, selection: Range) -> Vec<CodeAction> {
        if doc.language != "java" {
            return Vec::new();
        }
        let Some(iface) = parse::parse_java(&doc.text) else {
            return Vec::new();
        };
        // The index has not seen a mapper XML for this interface: offer the whole
        // set (mid-indexing, or genuinely none yet — the resolve confirms).
        let indexed = self.nav.indexed_mapper(&iface.fqn);
        let mut actions = Vec::new();
        if indexed.is_none() {
            actions.push(CodeAction {
                title: t("Create Mapper XML"),
                kind: None,
                preferred: false,
                edits: Vec::new(),
                data: Some("xml".into()),
            });
        }
        let Some(cursor) = offset(&doc.text, selection.start) else {
            return actions;
        };
        let Some(method) = iface
            .methods
            .iter()
            .find(|m| cursor >= m.extent.0 && cursor < m.extent.1)
        else {
            return actions;
        };
        if method.kind.is_some() && !method.default_method {
            let statement_present = indexed
                .as_ref()
                .is_some_and(|(_, statements)| statements.contains(&method.name));
            if !statement_present {
                actions.push(CodeAction {
                    title: t_with(
                        "Generate XML Statement for {name}",
                        &[("name", &method.name)],
                    ),
                    kind: None,
                    preferred: false,
                    edits: Vec::new(),
                    data: Some(format!("statement:{}", method.name)),
                });
            }
        }
        if !generate::param_annotation_edits(&iface, Some(method)).is_empty() {
            actions.push(CodeAction {
                title: t_with(
                    "Add @Param annotations to {name}",
                    &[("name", &method.name)],
                ),
                kind: None,
                preferred: false,
                edits: Vec::new(),
                data: Some(format!("param:{}", method.name)),
            });
        }
        actions
    }

    /// Resolves the three actions. What the mapper already has is jumped to
    /// (`host::open_location`); what it lacks is created — the statement lands
    /// in the mapper XML through a multi-file edit, so it stays undoable, while
    /// a fresh mapper XML is written directly (nothing to undo yet).
    fn resolve_code_action(
        &mut self,
        doc: &Document,
        action: CodeAction,
    ) -> Result<CodeAction, Error> {
        let Some(data) = &action.data else {
            return Ok(action);
        };
        let Some(iface) = parse::parse_java(&doc.text) else {
            return Ok(spent(&action));
        };
        let Some(project) = host::project_of(&doc.path) else {
            host::notify(
                NotificationLevel::Warning,
                "MyBatis Duo",
                &t("Open the mapper's project first"),
            );
            return Ok(spent(&action));
        };
        let root = project.root()?.trim_end_matches('/').to_owned();
        match data.as_str() {
            // The mapper XML: jump to its namespace when the interface's mapper
            // exists, else create it at the Spring convention path. A file that is
            // already there but maps something else is never overwritten.
            "xml" => {
                if let Some((path, xml)) = self.nav.xml_for(&project, &iface.fqn) {
                    // The jump needs an invocation with the workspace root, which
                    // the resolve does not carry on upstream hosts: offer it as the
                    // notification's action, run in the window's own context.
                    let line = xml
                        .namespace
                        .as_ref()
                        .map(|n| n.range.start.line)
                        .unwrap_or(0);
                    host::notify_with_actions(
                        NotificationLevel::Info,
                        "MyBatis Duo",
                        &t_with("The mapper XML already exists ({rel})", &[("rel", &path)]),
                        &[NotificationAction {
                            label: t("Open the mapper XML"),
                            command: "goto-mapper-xml".into(),
                            argument: Some(format!("{path}|{line}|0")),
                        }],
                    )?;
                    return Ok(spent(&action));
                }
                let rel = format!(
                    "src/main/resources/{}/{}.xml",
                    iface.pkg.replace('.', "/"),
                    iface.name
                );
                let target = format!("{root}/{rel}");
                if project.exists(&target).unwrap_or(true) {
                    host::notify(
                        NotificationLevel::Warning,
                        "MyBatis Duo",
                        &t_with("{rel} already exists", &[("rel", &rel)]),
                    );
                    return Ok(spent(&action));
                }
                if let Err(e) = project.write_file(&target, &generate::mapper_xml_content(&iface)) {
                    host::notify(
                        NotificationLevel::Warning,
                        "MyBatis Duo",
                        &format!("{target}: {e}"),
                    );
                    return Ok(spent(&action));
                }
                host::notify(
                    NotificationLevel::Info,
                    "MyBatis Duo",
                    &t_with("Created {rel}", &[("rel", &rel)]),
                );
                Ok(spent(&action))
            }
            // The statement: jump to it when the XML has it; append it through the
            // host's edit when the XML exists without it; create the XML with just
            // this statement when there is none.
            data if data.starts_with("statement:") => {
                let method_name = &data["statement:".len()..];
                let Some(method) = iface.methods.iter().find(|m| &m.name == method_name) else {
                    return Ok(spent(&action));
                };
                let Some(&inferred) = method.kind.as_ref() else {
                    host::notify(
                        NotificationLevel::Warning,
                        "MyBatis Duo",
                        &t(
                            "This method carries SQL annotations or is a default method — it needs no XML statement",
                        ),
                    );
                    return Ok(spent(&action));
                };
                let Some((xml_path, xml)) = self.nav.xml_for(&project, &iface.fqn) else {
                    let rel = format!(
                        "src/main/resources/{}/{}.xml",
                        iface.pkg.replace('.', "/"),
                        iface.name
                    );
                    let target = format!("{root}/{rel}");
                    if project.exists(&target).unwrap_or(true) {
                        host::notify(
                            NotificationLevel::Warning,
                            "MyBatis Duo",
                            &t_with("{rel} already exists", &[("rel", &rel)]),
                        );
                        return Ok(spent(&action));
                    }
                    if let Err(e) = project.write_file(
                        &target,
                        &generate::mapper_xml_with_statement(method, &iface, inferred),
                    ) {
                        host::notify(
                            NotificationLevel::Warning,
                            "MyBatis Duo",
                            &format!("{target}: {e}"),
                        );
                        return Ok(spent(&action));
                    }
                    host::notify(
                        NotificationLevel::Info,
                        "MyBatis Duo",
                        &t_with(
                            "Created {rel} with <{method}>",
                            &[("rel", &rel), ("method", &method.name)],
                        ),
                    );
                    return Ok(spent(&action));
                };
                if let Some(s) = xml.statements.iter().find(|s| &s.name == method_name) {
                    // The same: the button jumps with the window's own context.
                    host::notify_with_actions(
                        NotificationLevel::Info,
                        "MyBatis Duo",
                        &t_with(
                            "The statement already exists on line {line}",
                            &[("line", &(s.range.start.line + 1).to_string())],
                        ),
                        &[NotificationAction {
                            label: t("Go to the statement"),
                            command: "goto-mapper-xml".into(),
                            argument: Some(format!(
                                "{xml_path}|{}|{}",
                                s.range.start.line, s.range.start.character
                            )),
                        }],
                    )?;
                    return Ok(spent(&action));
                }
                let text = project.read_file(&xml_path)?;
                let statement = generate::statement_xml(method, &iface, inferred);
                let (from, to, insert) = generate::append_statement_edit(&text, &statement);
                host::notify(
                    NotificationLevel::Info,
                    "MyBatis Duo",
                    &t_with(
                        "Generated <{method}> in {path}",
                        &[("method", &method.name), ("path", &xml_path)],
                    ),
                );
                Ok(CodeAction {
                    title: action.title.clone(),
                    kind: action.kind.clone(),
                    preferred: true,
                    edits: vec![(
                        xml_path,
                        vec![TextEdit {
                            range: Range::new(position(&text, from), position(&text, to)),
                            text: insert,
                        }],
                    )],
                    data: None,
                })
            }
            // The annotations: apply the missing ones to the interface; a fully
            // annotated method stays untouched (the resolve returns no edits).
            data if data.starts_with("param:") => {
                let method_name = &data["param:".len()..];
                let Some(method) = iface.methods.iter().find(|m| &m.name == method_name) else {
                    return Ok(spent(&action));
                };
                let mut edits = generate::param_annotation_edits(&iface, Some(method));
                if edits.is_empty() {
                    // Every parameter is annotated already: nothing to do, not even
                    // the import (the user asked exactly for this).
                    host::notify(
                        NotificationLevel::Info,
                        "MyBatis Duo",
                        &t_with(
                            "All parameters of {name} already have @Param",
                            &[("name", &method.name)],
                        ),
                    );
                    return Ok(spent(&action));
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
                Ok(CodeAction {
                    title: action.title.clone(),
                    kind: action.kind.clone(),
                    preferred: true,
                    edits: vec![(doc.path.clone(), text_edits)],
                    data: None,
                })
            }
            _ => Ok(action),
        }
    }
}

/// An action whose resolve ran: the `data` reference is consumed either way.
fn spent(action: &CodeAction) -> CodeAction {
    CodeAction {
        title: action.title.clone(),
        kind: action.kind.clone(),
        preferred: action.preferred,
        edits: Vec::new(),
        data: None,
    }
}

register_plugin!(MybatisPlus, providers, indexing);

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
    fn the_three_actions_stay_listed_and_the_param_one_resolves() {
        let host = MockHost::install();
        host.open_editor(
            "/project/src/UserMapper.java",
            "java",
            MAPPER,
            &[(5, 12, 5, 12)],
        );
        let doc = Document {
            path: "/project/src/UserMapper.java".into(),
            language: "java".into(),
            version: 1,
            text: MAPPER.into(),
        };
        let actions = MybatisPlus::default().code_actions(
            &doc,
            host.invocation().editor().unwrap().selections().unwrap()[0],
        );
        // Create XML, Generate Statement, Add @Param — all three, all lazy.
        assert_eq!(actions.len(), 3, "{actions:?}");
        assert!(actions.iter().all(|a| a.edits.is_empty()));
        assert_eq!(
            actions
                .iter()
                .filter_map(|a| a.data.as_deref())
                .collect::<Vec<_>>(),
            ["xml", "statement:findByName", "param:findByName"]
        );

        // Resolving the @Param action applies the two annotations and the import
        // to the interface itself.
        let param = actions
            .iter()
            .find(|a| a.data.as_deref() == Some("param:findByName"))
            .unwrap()
            .clone();
        let resolved = MybatisPlus::default()
            .resolve_code_action(&doc, param)
            .unwrap();
        assert_eq!(resolved.edits.len(), 1, "{resolved:?}");
        assert_eq!(resolved.edits[0].0, "/project/src/UserMapper.java");
        assert_eq!(resolved.edits[0].1.len(), 3, "two annotations + the import");
        assert!(
            resolved.data.is_none(),
            "the reference is consumed on resolve"
        );

        // An already-annotated method offers no @Param action at all.
        let annotated = MAPPER.replace(
            "String name, int size",
            "@Param(\"name\") String name, @Param(\"size\") int size",
        );
        let doc = Document {
            text: annotated,
            ..doc
        };
        let actions = MybatisPlus::default().code_actions(
            &doc,
            host.invocation().editor().unwrap().selections().unwrap()[0],
        );
        assert_eq!(actions.len(), 2, "{actions:?}");
        assert!(
            actions
                .iter()
                .all(|a| a.data.as_deref() != Some("param:findByName"))
        );
    }

    #[test]
    fn missing_params_publish_diagnostics_that_clear_when_fixed() {
        let host = MockHost::install();
        host.open_editor(
            "/project/src/UserMapper.java",
            "java",
            MAPPER,
            &[(5, 12, 5, 12)],
        );
        let indexed_doc = openidea_plugin::indexing::Document {
            path: "/project/src/UserMapper.java".into(),
            text: MAPPER.into(),
            symbols: Vec::new(),
        };
        MybatisPlus::default().index(&indexed_doc);
        let diags = host.diagnostics();
        let items = diags
            .get("/project/src/UserMapper.java")
            .expect("the mapper's diagnostics are published");
        // Both parameters lack @Param (a two-parameter method needs them).
        assert_eq!(items.len(), 2, "{items:?}");
        assert!(items[0].message.contains("no @Param"));

        // Fixing the parameters clears the warnings on the next pass.
        let fixed = MAPPER.replace(
            "String name, int size",
            "@Param(\"name\") String name, @Param(\"size\") int size",
        );
        let fixed_doc = openidea_plugin::indexing::Document {
            path: "/project/src/UserMapper.java".into(),
            text: fixed,
            symbols: Vec::new(),
        };
        MybatisPlus::default().index(&fixed_doc);
        assert!(
            host.diagnostics()
                .get("/project/src/UserMapper.java")
                .is_none_or(|items| items.is_empty())
        );
    }

    #[test]
    fn the_actions_filter_on_the_indexed_mapper_state() {
        let host = MockHost::install();
        host.open_editor(
            "/project/src/UserMapper.java",
            "java",
            MAPPER,
            &[(5, 12, 5, 12)],
        );
        let doc = Document {
            path: "/project/src/UserMapper.java".into(),
            language: "java".into(),
            version: 1,
            text: MAPPER.into(),
        };
        let mut plugin = MybatisPlus::default();
        let fqn = "com.example.mapper.UserMapper";

        // The index has seen the mapper XML with the statement already: only the
        // @Param action remains (findByName lacks the annotations).
        plugin
            .nav
            .mapper_files
            .insert(fqn.into(), "/project/src/UserMapper.xml".into());
        plugin.nav.mapper_statements.insert(
            fqn.into(),
            std::collections::BTreeSet::from(["findByName".into()]),
        );
        let actions = plugin.code_actions(
            &doc,
            host.invocation().editor().unwrap().selections().unwrap()[0],
        );
        assert_eq!(actions.len(), 1, "{actions:?}");
        assert_eq!(actions[0].data.as_deref(), Some("param:findByName"));

        // Without the statement in the XML, the generate action is offered too
        // (and the create stays hidden while the XML exists).
        plugin
            .nav
            .mapper_statements
            .insert(fqn.into(), std::collections::BTreeSet::new());
        let actions = plugin.code_actions(
            &doc,
            host.invocation().editor().unwrap().selections().unwrap()[0],
        );
        assert_eq!(actions.len(), 2, "{actions:?}");
        assert_eq!(actions[0].data.as_deref(), Some("statement:findByName"));
        assert_eq!(actions[1].data.as_deref(), Some("param:findByName"));

        // No mapper XML seen at all: the create action comes back as well.
        plugin.nav.mapper_files.clear();
        let actions = plugin.code_actions(
            &doc,
            host.invocation().editor().unwrap().selections().unwrap()[0],
        );
        assert_eq!(actions.len(), 3, "{actions:?}");
        assert_eq!(actions[0].data.as_deref(), Some("xml"));
    }

    #[test]
    fn a_single_parameter_method_gets_param_annotations_too() {
        // MyBatisX asks for @Param on every mapper parameter; the previous
        // "two parameters only" rule skipped `selectByProdOrderNo(String)` and
        // then claimed everything was annotated already.
        const SINGLE: &str = r#"
package com.steerinfo.pcmplnproductionorder.mapper;

public interface PcmPlnProductionorderMapper {
    PcmPlnProductionorder selectByProdOrderNo(String prodOrderNo);
}
"#;
        let host = MockHost::install();
        host.open_editor(
            "/project/src/PcmPlnProductionorderMapper.java",
            "java",
            SINGLE,
            &[(4, 20, 4, 20)],
        );
        let doc = Document {
            path: "/project/src/PcmPlnProductionorderMapper.java".into(),
            language: "java".into(),
            version: 1,
            text: SINGLE.into(),
        };
        let actions = MybatisPlus::default().code_actions(
            &doc,
            host.invocation().editor().unwrap().selections().unwrap()[0],
        );
        let param = actions
            .iter()
            .find(|a| a.data.as_deref() == Some("param:selectByProdOrderNo"))
            .expect("the single parameter is offered @Param too")
            .clone();
        let resolved = MybatisPlus::default()
            .resolve_code_action(&doc, param)
            .unwrap();
        assert_eq!(resolved.edits.len(), 1, "{resolved:?}");
        assert!(
            resolved.edits[0].1[0]
                .text
                .contains("@Param(\"prodOrderNo\")")
        );
    }

    #[test]
    fn the_statement_action_resolves_through_the_project() {
        let host = MockHost::install();
        host.open_editor(
            "/project/src/UserMapper.java",
            "java",
            MAPPER,
            &[(5, 12, 5, 12)],
        );
        let doc = Document {
            path: "/project/src/UserMapper.java".into(),
            language: "java".into(),
            version: 1,
            text: MAPPER.into(),
        };
        let actions = MybatisPlus::default().code_actions(
            &doc,
            host.invocation().editor().unwrap().selections().unwrap()[0],
        );
        let statement = actions
            .iter()
            .find(|a| a.data.is_some())
            .expect("the statement action")
            .clone();

        // No mapper XML in the project yet: the action comes back without edits
        // and a notice points at `Create Mapper XML` (the full append is covered
        // by the `generate-statement` command tests through the same helpers).
        let resolved = MybatisPlus::default()
            .resolve_code_action(&doc, statement)
            .unwrap();
        assert!(resolved.edits.is_empty(), "{resolved:?}");
        assert!(
            resolved.data.is_none(),
            "the reference is consumed on resolve"
        );
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

    #[test]
    fn a_console_invocation_converts_the_passed_text() {
        // The host's Run console passes its selection as the command's argument,
        // with no editor focused: the arg is what gets converted.
        let host = MockHost::install();
        let log =
            "==>  Preparing: select id from users where code = ?\n==> Parameters: 00123(String)\n";
        let out = MybatisPlus::default()
            .run_command("convert-sql-log", &host.invocation(), Some(log))
            .unwrap();
        assert_eq!(
            out.as_deref(),
            Some("select id from users where code = '00123';")
        );
        assert_eq!(
            host.clipboard().as_deref(),
            Some("select id from users where code = '00123';")
        );
    }

    #[test]
    fn a_string_typed_number_reaches_the_clipboard_quoted() {
        // Issue #2 end to end: the command and the clipboard, not just `sqllog::convert`.
        let host = MockHost::install();
        let log =
            "==>  Preparing: select * from users where code = ?\n==> Parameters: 00123(String)\n";
        host.open_editor("/p/a.log", "log", log, &[(0, 0, 2, 0)]);
        let out = MybatisPlus::default()
            .run_command("convert-sql-log", &host.invocation(), None)
            .unwrap();
        assert_eq!(
            out.as_deref(),
            Some("select * from users where code = '00123';")
        );
        assert_eq!(
            host.clipboard().as_deref(),
            Some("select * from users where code = '00123';")
        );
    }

    const MAPPER_XML: &str = "<?xml version=\"1.0\"?>\n\
                              <mapper namespace=\"com.example.mapper.UserMapper\">\n  \
                              <select id=\"findByName\">\n    select * from users where name = #{name} and size = #{size}\n  </select>\n</mapper>\n";

    /// A mapper XML found by the namespace search, as the host's Find in Files answers it.
    fn add_mapper(host: &MockHost) {
        host.add_file("src/main/resources/mapper/UserMapper.xml", MAPPER_XML);
        host.on_search(move |query, _| {
            assert!(query.regex && query.include == ["**/*.xml"], "{query:?}");
            let namespace = query
                .pattern
                .split(r#"["']"#)
                .nth(1)
                .unwrap_or_default()
                .replace('\\', "");
            let line = MAPPER_XML
                .lines()
                .position(|l| l.contains(&format!("namespace=\"{namespace}\"")));
            let matches = line
                .map(|line| openidea_plugin::TextMatch {
                    location: openidea_plugin::Location {
                        path: "/project/src/main/resources/mapper/UserMapper.xml".into(),
                        range: Range::new(
                            Position::new(line as u32, 0),
                            Position::new(line as u32, 0),
                        ),
                    },
                    line: String::new(),
                })
                .into_iter()
                .collect();
            Ok(openidea_plugin::TextSearch {
                matches,
                limit_hit: false,
            })
        });
    }

    #[test]
    fn goto_mapper_xml_opens_the_statement() {
        let host = MockHost::install();
        host.add_file("src/main/java/com/example/mapper/UserMapper.java", MAPPER);
        add_mapper(&host);
        host.open_editor(
            "/project/src/main/java/com/example/mapper/UserMapper.java",
            "java",
            MAPPER,
            &[(5, 20, 5, 20)],
        );
        let out = MybatisPlus::default()
            .run_command("goto-mapper-xml", &host.invocation(), None)
            .unwrap();
        assert_eq!(
            out.as_deref(),
            Some("/project/src/main/resources/mapper/UserMapper.xml")
        );
        assert_eq!(
            host.opened_locations(),
            [(
                "/project/src/main/resources/mapper/UserMapper.xml".to_owned(),
                2,
                14
            )]
        );
    }

    #[test]
    fn index_publishes_namespace_and_xml_parameter_anchors() {
        use openidea_plugin::indexing::Document as IndexDocument;
        let _host = MockHost::install();
        let mut plugin = MybatisPlus::default();
        let java = IndexDocument {
            path: "/p/UserMapper.java".into(),
            text: MAPPER.into(),
            symbols: vec![],
        };
        let edges = plugin.index(&java);
        assert!(
            edges
                .definitions
                .iter()
                .any(|a| a.key == "mapper:com.example.mapper.UserMapper"),
            "the interface declaration is a definition: {:?}",
            edges
                .definitions
                .iter()
                .map(|a| a.key.as_str())
                .collect::<Vec<_>>()
        );
        assert!(
            edges
                .definitions
                .iter()
                .any(|a| a.key.ends_with("#findByName::param::name")),
            "each method parameter is a definition"
        );

        let xml = IndexDocument {
            path: "/p/UserMapper.xml".into(),
            text: MAPPER_XML.into(),
            symbols: vec![],
        };
        let edges = plugin.index(&xml);
        assert!(
            edges
                .references
                .iter()
                .any(|a| a.key == "mapper:com.example.mapper.UserMapper"),
            "the namespace reaches the interface declaration"
        );
        for name in ["name", "size"] {
            assert!(
                edges
                    .references
                    .iter()
                    .any(|a| a.key.ends_with(&format!("#findByName::param::{name}"))),
                "#{{{name}}} reaches the method parameter: {:?}",
                edges
                    .references
                    .iter()
                    .map(|a| a.key.as_str())
                    .collect::<Vec<_>>()
            );
        }
    }
}
