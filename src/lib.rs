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
use openidea_plugin::{
    Error, Invocation, NotificationLevel, PickItem, Plugin, Position, Range, TextEdit, host, register_plugin,
};

#[derive(Default)]
struct MybatisPlus {
    /// The ported official navigator (namespace→XML cache included).
    nav: nav::Navigator,
}

impl Plugin for MybatisPlus {
    fn run_command(&mut self, id: &str, ctx: &Invocation, _arg: Option<&str>) -> Result<Option<String>, Error> {
        match id {
            "convert-sql-log" => convert_sql_log(ctx),
            "generate-statement" => generate_statement(ctx),
            "create-mapper-xml" => create_mapper_xml(ctx),
            "goto-mapper-xml" => goto_mapper_xml(&mut self.nav, ctx),
            other => Err(Error::NotFound(format!("command {other}"))),
        }
    }
}

/// `Go to Mapper XML`: the caret on a mapper method opens its XML statement (or
/// `@Param`, result map, `<include>` target); an XML statement id opens the
/// method. Several targets are offered in a picker. This is the direct jump the
/// host's ⌘B no longer gives on a declaration (it shows usages there).
fn goto_mapper_xml(nav: &mut nav::Navigator, ctx: &Invocation) -> Result<Option<String>, Error> {
    let editor = ctx.editor().ok_or_else(|| Error::Invalid(t("no file is open in the editor")))?;
    let text = editor.document()?;
    let doc = Document {
        path: editor.path(),
        language: editor.language(),
        version: text.version,
        text: text.text,
    };
    let selections = editor.selections()?;
    let Some(selection) = selections.first() else { return Ok(None) };
    let Some(cursor) = offset(&doc.text, selection.start) else { return Ok(None) };
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
                let file = target.path.rsplit(['/', '\\']).next().unwrap_or(&target.path).to_owned();
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
    host::open_location(&target.path, target.range.start.line, target.range.start.character)?;
    Ok(Some(target.path.clone()))
}

/// `Copy MyBatis SQL` for the editor selection: the converter is pure, the
/// result goes to the clipboard.
fn convert_sql_log(ctx: &Invocation) -> Result<Option<String>, Error> {
    let editor = ctx.editor().ok_or_else(|| Error::Invalid(t("no file is open in the editor")))?;
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
            &t("The selection has no MyBatis SQL log to convert"),
        );
        return Ok(None);
    };
    host::copy_to_clipboard(&sql)?;
    host::notify(NotificationLevel::Info, "MyBatis SQL", &t("The converted SQL is on the clipboard"));
    Ok(Some(sql))
}

/// `Create Mapper Statement`: the caret sits in a mapper method; the statement
/// goes into the matching mapper XML, or a new one is created after a
/// confirmation when none exists.
fn generate_statement(ctx: &Invocation) -> Result<Option<String>, Error> {
    let editor = ctx.editor().ok_or_else(|| Error::Invalid(t("no file is open in the editor")))?;
    if editor.language() != "java" {
        host::notify(NotificationLevel::Warning, "MyBatis Duo", &t("The caret is not in a Java file"));
        return Ok(None);
    }
    let doc = editor.document()?;
    let selections = editor.selections()?;
    let cursor = selections.first().and_then(|r| offset(&doc.text, r.start)).unwrap_or(0);
    let Some(iface) = parse::parse_java(&doc.text) else {
        host::notify(NotificationLevel::Warning, "MyBatis Duo", &t("The file is not a mapper interface"));
        return Ok(None);
    };
    let method = iface
        .methods
        .iter()
        .find(|m| cursor >= m.extent.0 && cursor < m.extent.1);
    let Some(method) = method else {
        host::notify(NotificationLevel::Warning, "MyBatis Duo", &t("Put the caret in a mapper method"));
        return Ok(None);
    };
    let Some(kind) = method.kind else {
        host::notify(
            NotificationLevel::Warning,
            "MyBatis Duo",
            &t("This method carries SQL annotations or is a default method — it needs no XML statement"),
        );
        return Ok(None);
    };
    let project = ctx.project().ok_or_else(|| Error::Invalid(t("no project is open")))?;
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
                &t_with("{name}#{method} already has an XML statement", &[("name", &iface.name), ("method", &method.name)]),
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
                    &t_with("Created {kind} <{method}> in {path}", &[("kind", kind.as_str()), ("method", &method.name), ("path", &path)]),
                );
                return Ok(Some(path));
            }
            Err(Error::Invalid(_)) => {
                host::notify(
                    NotificationLevel::Warning,
                    "MyBatis Duo",
                    &t_with("Save {path} first: it has unsaved changes", &[("path", &path)]),
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
        &t_with("No mapper XML for {fqn} — use 'MyBatis Duo: Create Mapper XML' first", &[("fqn", &iface.fqn)]),
    );
    Ok(None)
}

/// `Create Mapper XML`: the whole file for the interface, at the Spring
/// convention path under `src/main/resources`.
fn create_mapper_xml(ctx: &Invocation) -> Result<Option<String>, Error> {
    let editor = ctx.editor().ok_or_else(|| Error::Invalid(t("no file is open in the editor")))?;
    if editor.language() != "java" {
        host::notify(NotificationLevel::Warning, "MyBatis Duo", &t("The caret is not in a Java file"));
        return Ok(None);
    }
    let doc = editor.document()?;
    let Some(iface) = parse::parse_java(&doc.text) else {
        host::notify(NotificationLevel::Warning, "MyBatis Duo", &t("The file is not a mapper interface"));
        return Ok(None);
    };
    let project = ctx.project().ok_or_else(|| Error::Invalid(t("no project is open")))?;
    let root = project.root()?;
    let rel = format!("src/main/resources/{}/{}.xml", iface.pkg.replace('.', "/"), iface.name);
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
    host::notify(NotificationLevel::Info, "MyBatis Duo", &t_with("Created {rel}", &[("rel", &rel)]));
    Ok(Some(target))
}

impl DocumentIndexer for MybatisPlus {
    /// Background indexing: the host feeds documents in batches (the 5 s
    /// command budget) and answers gutter arrows and Go to Definition from
    /// the graph, so navigation never runs inside the 500 ms edit budget.
    fn index(&mut self, doc: &openidea_plugin::indexing::Document) -> openidea_plugin::indexing::Edges {
        self.nav.index(doc)
    }
}

impl Providers for MybatisPlus {
    /// Statement ids, `<sql>` refids, `#{…}` parameters, result-map and
    /// annotation-SQL parameter completion. (Gutter arrows and Go to
    /// Definition are answered from the index graph — see `DocumentIndexer`.)
    fn completion(&mut self, doc: &Document, at: Position, trigger: Option<&str>) -> Vec<CompletionItem> {
        self.nav.completion(doc, at, trigger)
    }

    /// `Add @Param annotations` as a quickfix on the mapper method under the
    /// caret; edits stay in the current file (annotations + the import). The
    /// statement generation is offered the same way, but its edits land in the
    /// mapper XML and need the project to find that file, so they are computed
    /// in `resolve_code_action` (the command budget) after the user picks it.
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
        let mut actions = Vec::new();
        if method.kind.is_some() && !method.default_method {
            actions.push(CodeAction {
                title: t_with("Generate XML Statement for {name}", &[("name", &method.name)]),
                kind: None,
                preferred: false,
                edits: Vec::new(),
                data: Some(format!("statement:{}", method.name)),
            });
        }
        let mut edits = generate::param_annotation_edits(&iface, Some(method));
        if let Some(import) = generate::import_edit(&doc.text) {
            edits.push(import);
        }
        if !edits.is_empty() {
            let text_edits: Vec<TextEdit> = edits
                .iter()
                .map(|(from, to, insert)| TextEdit {
                    range: Range::new(position(&doc.text, *from), position(&doc.text, *to)),
                    text: insert.clone(),
                })
                .collect();
            actions.push(CodeAction {
                title: t_with("Add @Param annotations to {name}", &[("name", &method.name)]),
                kind: Some("quickfix".into()),
                preferred: true,
                edits: vec![(doc.path.clone(), text_edits)],
                data: None,
            });
        }
        actions
    }

    /// Resolves the statement generation: finds the interface's mapper XML in
    /// the project and returns the edit that appends the statement before
    /// `</mapper>` (the host applies the multi-file edit). Everything else
    /// comes back untouched.
    fn resolve_code_action(&mut self, doc: &Document, action: CodeAction) -> Result<CodeAction, Error> {
        let Some(data) = &action.data else { return Ok(action) };
        let Some(method_name) = data.strip_prefix("statement:") else { return Ok(action) };
        let Some(iface) = parse::parse_java(&doc.text) else { return Ok(action) };
        let Some(method) = iface.methods.iter().find(|m| &m.name == method_name) else {
            return Ok(action);
        };
        let Some(kind) = &method.kind else {
            host::notify(
                NotificationLevel::Warning,
                "MyBatis Duo",
                &t("This method carries SQL annotations or is a default method — it needs no XML statement"),
            );
            return Ok(action);
        };
        let Some(project) = host::project_of(&doc.path) else {
            host::notify(NotificationLevel::Warning, "MyBatis Duo", &t("Open the mapper's project first"));
            return Ok(action);
        };
        // The navigation index already maps every mapper XML (the gutters jump with
        // it), so ask it for the namespace instead of a `*Mapper.xml` glob that
        // misses once a project outgrows the search's result cap.
        let Some((xml_path, xml)) = self.nav.xml_for(&project, &iface.fqn) else {
            host::notify(
                NotificationLevel::Warning,
                "MyBatis Duo",
                &t_with("No mapper XML for {fqn} — use 'MyBatis Duo: Create Mapper XML' first", &[("fqn", &iface.fqn)]),
            );
            return Ok(CodeAction {
                title: action.title.clone(),
                kind: action.kind.clone(),
                preferred: action.preferred,
                edits: Vec::new(),
                data: None,
            });
        };
        if xml.statements.iter().any(|s| &s.name == method_name) {
            host::notify(
                NotificationLevel::Info,
                "MyBatis Duo",
                &t_with("{name}#{method} already has an XML statement", &[("name", &iface.name), ("method", &method.name)]),
            );
            return Ok(action);
        }
        let statement = generate::statement_xml(method, &iface);
        let text = project.read_file(&xml_path)?;
        let (from, to, insert) = generate::append_statement_edit(&text, &statement);
        let edits = vec![TextEdit {
            range: Range::new(position(&text, from), position(&text, to)),
            text: insert,
        }];
        Ok(CodeAction {
            title: action.title.clone(),
            kind: action.kind.clone(),
            preferred: true,
            edits: vec![(xml_path, edits)],
            data: None,
        })
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
        assert_eq!(actions.len(), 2, "{actions:?}");
        let statement = actions.iter().find(|a| a.data.is_some()).expect("the statement action");
        assert_eq!(statement.data.as_deref(), Some("statement:findByName"));
        assert!(statement.edits.is_empty(), "the edits resolve after the pick");
        let param = actions
            .iter()
            .find(|a| a.title.contains("findByName") && a.data.is_none())
            .expect("the @Param quickfix");
        assert_eq!(param.edits[0].1.len(), 3, "two annotations + the import");
    }

    #[test]
    fn the_statement_action_resolves_through_the_project() {
        let host = MockHost::install();
        host.open_editor("/project/src/UserMapper.java", "java", MAPPER, &[(5, 12, 5, 12)]);
        let doc = Document {
            path: "/project/src/UserMapper.java".into(),
            language: "java".into(),
            version: 1,
            text: MAPPER.into(),
        };
        let actions = MybatisPlus::default().code_actions(&doc, host.invocation().editor().unwrap().selections().unwrap()[0]);
        let statement = actions.iter().find(|a| a.data.is_some()).expect("the statement action").clone();

        // No mapper XML in the project yet: the action comes back without edits
        // and a notice points at `Create Mapper XML` (the full append is covered
        // by the `generate-statement` command tests through the same helpers).
        let resolved = MybatisPlus::default().resolve_code_action(&doc, statement).unwrap();
        assert!(resolved.edits.is_empty(), "{resolved:?}");
        assert!(resolved.data.is_none(), "the reference is consumed on resolve");
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
    fn a_string_typed_number_reaches_the_clipboard_quoted() {
        // Issue #2 end to end: the command and the clipboard, not just `sqllog::convert`.
        let host = MockHost::install();
        let log = "==>  Preparing: select * from users where code = ?\n==> Parameters: 00123(String)\n";
        host.open_editor("/p/a.log", "log", log, &[(0, 0, 2, 0)]);
        let out = MybatisPlus::default().run_command("convert-sql-log", &host.invocation(), None).unwrap();
        assert_eq!(out.as_deref(), Some("select * from users where code = '00123';"));
        assert_eq!(host.clipboard().as_deref(), Some("select * from users where code = '00123';"));
    }

    const MAPPER_XML: &str = "<?xml version=\"1.0\"?>\n\
                              <mapper namespace=\"com.example.mapper.UserMapper\">\n  \
                              <select id=\"findByName\">\n    select * from users where name = #{name} and size = #{size}\n  </select>\n</mapper>\n";

    /// A mapper XML found by the namespace search, as the host's Find in Files answers it.
    fn add_mapper(host: &MockHost) {
        host.add_file("src/main/resources/mapper/UserMapper.xml", MAPPER_XML);
        host.on_search(move |query, _| {
            assert!(query.regex && query.include == ["**/*.xml"], "{query:?}");
            let namespace = query.pattern.split(r#"["']"#).nth(1).unwrap_or_default().replace('\\', "");
            let line = MAPPER_XML.lines().position(|l| l.contains(&format!("namespace=\"{namespace}\"")));
            let matches = line
                .map(|line| openidea_plugin::TextMatch {
                    location: openidea_plugin::Location {
                        path: "/project/src/main/resources/mapper/UserMapper.xml".into(),
                        range: Range::new(Position::new(line as u32, 0), Position::new(line as u32, 0)),
                    },
                    line: String::new(),
                })
                .into_iter()
                .collect();
            Ok(openidea_plugin::TextSearch { matches, limit_hit: false })
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
        let out = MybatisPlus::default().run_command("goto-mapper-xml", &host.invocation(), None).unwrap();
        assert_eq!(out.as_deref(), Some("/project/src/main/resources/mapper/UserMapper.xml"));
        assert_eq!(
            host.opened_locations(),
            [("/project/src/main/resources/mapper/UserMapper.xml".to_owned(), 2, 14)]
        );
    }

    #[test]
    fn index_publishes_namespace_and_xml_parameter_anchors() {
        use openidea_plugin::indexing::Document as IndexDocument;
        let mut plugin = MybatisPlus::default();
        let java = IndexDocument { path: "/p/UserMapper.java".into(), text: MAPPER.into(), symbols: vec![] };
        let edges = plugin.index(&java);
        assert!(
            edges.definitions.iter().any(|a| a.key == "mapper:com.example.mapper.UserMapper"),
            "the interface declaration is a definition: {:?}",
            edges.definitions.iter().map(|a| a.key.as_str()).collect::<Vec<_>>()
        );
        assert!(
            edges.definitions.iter().any(|a| a.key.ends_with("#findByName::param::name")),
            "each method parameter is a definition"
        );

        let xml = IndexDocument { path: "/p/UserMapper.xml".into(), text: MAPPER_XML.into(), symbols: vec![] };
        let edges = plugin.index(&xml);
        assert!(
            edges.references.iter().any(|a| a.key == "mapper:com.example.mapper.UserMapper"),
            "the namespace reaches the interface declaration"
        );
        for name in ["name", "size"] {
            assert!(
                edges.references.iter().any(|a| a.key.ends_with(&format!("#findByName::param::{name}"))),
                "#{{{name}}} reaches the method parameter: {:?}",
                edges.references.iter().map(|a| a.key.as_str()).collect::<Vec<_>>()
            );
        }
    }
}