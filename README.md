# MyBatis Duo

A plugin for [OpenIDEA](https://github.com/l0ng-ai/openidea) that adds what the
official MyBatis Navigator plugin lacks:

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

Navigation (gutter arrows, Go to Definition, statement-id and parameter
completion) is the official `openidea.mybatis` plugin's job; install both.
A console entry for the SQL conversion lands once the host ships the console
context menu API.

**Known limitation:** when the project is open as a *project window group*
(workspace), the plugin API exposes only the group root — its members' files
are outside it, so statement generation and XML creation report that no mapper
XML is reachable. SQL-log conversion works (it reads the editor buffer). Open a
member as its own project, or wait for the host to expose member projects to
plugins.

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
