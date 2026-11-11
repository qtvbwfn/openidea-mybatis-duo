# MyBatis Duo

A plugin for [OpenIDEA](https://github.com/l0ng-ai/openidea) that provides the
whole MyBatis assist set in one install (requires OpenIDEA with plugin API 1.5,
i.e. v0.1.41 or newer):

- **Navigation both ways** — gutter arrows between a mapper interface and its
  XML statements and back, Go to Definition on namespaces, methods, statement
  ids, `#{…}` parameters, result maps and `<include>` refs, `#{}`/`${}`
  parameter completion, statement-id and `<sql>` refid completion, `@ResultMap`
  ids and annotation-SQL parameters (parsing ported from the official
  `openidea.mybatis` plugin 0.4.1, Apache-2.0).

  Navigation is served from a **background index**: the host feeds documents to
  the plugin in cancellable batches and answers gutter arrows and ⌘B from the
  project graph, so large projects (1,000+ XML files) never hit the live
  provider's time budget. Files are indexed as the project's file index reaches
  them — an opened file is indexed first, the rest catch up in the background.
- **Statement generation** — for a mapper method with no matching statement,
  create it in the mapper XML (the kind is inferred from the return type and
  refined by the method-name prefix). When the mapper XML does not exist yet,
  `MyBatis Duo: Create Mapper XML` writes the whole file at the Spring
  convention path (`src/main/resources/<package>/<Interface>.xml`).
- **`@Param` quickfix** — a multi-parameter mapper method missing `@Param`
  (or an empty `@Param()`) gets its annotations and import added (⌥↵).
- **SQL-log conversion** — with `==>  Preparing:` / `==> Parameters:` log text
  selected in an editor, copy runnable SQL: parameter values are substituted
  with proper quoting and escaping, and the statement text is kept exactly as
  logged.

**Disable the official MyBatis Navigator plugin** when Duo is installed: both
provide gutter arrows and completion on the same files, so keeping both shows
every arrow twice. A console entry for the SQL conversion lands once the host
ships the console context menu API.

**Known limitation:** when the project is open as a *project window group*
(workspace), the plugin API exposes only the group root — its members' files
are outside it, so statement generation and XML creation report that no mapper
XML is reachable. SQL-log conversion works (it reads the editor buffer). Open a
member as its own project, or wait for the host to expose member projects to
plugins.

## Translations

The plugin ships `locales/zh-CN.json`: the IDE translates the command titles
from it, and the plugin itself translates its notifications and the `@Param`
quickfix title by the UI language (`host::locale()`). Text without an entry
stays English; adding a language is a new `locales/<tag>.json`.

## Install

Build, then install from disk (Settings › Plugins › Install Plugin from Disk)
or drop the directory into `<project>/.openidea/plugins/` while developing.

```sh
rustup target add wasm32-wasip2   # once
cargo build --release --target wasm32-wasip2
mkdir -p dist
cp target/wasm32-wasip2/release/openidea_plugin_mybatis_duo.wasm dist/plugin.wasm
```

The manifest requests `project.read`, `project.write` and `clipboard`; the IDE
asks for consent on install.

## License

Apache-2.0. The parsing layer is adapted from the official OpenIDEA MyBatis
Navigator plugin (`plugins/official/mybatis`), which is Apache-2.0.
