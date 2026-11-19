# MyBatis Duo

A plugin for [OpenIDEA](https://github.com/l0ng-ai/openidea) with the MyBatis
assist set that the IDE does not ship: statement generation, completion, the
`@Param` quickfix and SQL-log conversion. Navigation is **not** part of it —
gutter arrows between a mapper interface and its XML and Go to Definition are
built into OpenIDEA v0.1.44+, so the plugin no longer duplicates them.

OpenIDEA 插件，提供 IDE 未内置的 MyBatis 辅助功能：生成 Mapper Statement、
补全、`@Param` 快速修复与 SQL 日志转换。导航**不**在其中——行槽箭头（Mapper
接口与 XML 互相跳转）和转到定义已由 OpenIDEA v0.1.44+ 内置，插件不再重复提供。

## Features

- **Statement generation** — for a mapper method with no matching statement,
  create it in the mapper XML (the kind is inferred from the return type and
  refined by the method-name prefix). When the mapper XML does not exist yet,
  `MyBatis Duo: Create Mapper XML` writes the whole file at the Spring
  convention path (`src/main/resources/<package>/<Interface>.xml`).
- **`@Param` quickfix** — a multi-parameter mapper method missing `@Param`
  (or an empty `@Param()`) gets its annotations and import added (⌥↵).
- **Completion** — statement ids from the interface's unmapped methods,
  `<sql>` ids for `<include refid>`, `#{}` parameters of the enclosing
  statement, `@ResultMap` ids (annotation `@Results` + XML `<resultMap>`)
  and parameters inside annotation SQL (parsing ported from the official
  `openidea.mybatis` plugin 0.4.1, Apache-2.0).
- **SQL-log conversion** — with `==>  Preparing:` / `==> Parameters:` log
  text selected in an editor, copy runnable SQL: parameter values are
  substituted with proper quoting and escaping, and the statement text is
  kept exactly as logged.

- **生成 Mapper Statement** — 为没有对应语句的 Mapper 方法在 Mapper XML 中
  创建语句（类型由返回类型推断，并按方法名前缀修正）。当 Mapper XML 尚不
  存在时，`MyBatis Duo: Create Mapper XML` 会按 Spring 约定路径
  （`src/main/resources/<package>/<Interface>.xml`）写出整个文件。
- **`@Param` 快速修复** — 缺少 `@Param`（或 `@Param()` 为空）的多参数 Mapper
  方法会自动补上注解和 import（⌥↵）。
- **补全** — 来自接口未映射方法的语句 id、`<include refid>` 用的 `<sql>` id、
  所在语句的 `#{}` 参数、`@ResultMap` id（注解 `@Results` + XML
  `<resultMap>`）以及注解 SQL 中的参数（解析层移植自官方
  `openidea.mybatis` 插件 0.4.1，Apache-2.0）。
- **SQL 日志转换** — 在编辑器中选中 `==>  Preparing:` / `==> Parameters:`
  日志文本，复制可运行的 SQL：参数值按正确的引号与转义替换，语句文本与日
  志保持完全一致。

## Requirements

OpenIDEA with plugin API 1.5 or newer (v0.1.41+). The built-in navigation this
plugin defers to needs v0.1.44+ (a fork version). Users of upstream OpenIDEA
without built-in MyBatis navigation can stay on 0.3.3, which still ships the
plugin-side gutter arrows and Go to Definition.

需要 OpenIDEA 插件 API 1.5 或更新（v0.1.41+）。本插件所依赖的内置导航需要
v0.1.44+（fork 版本）。没有内置 MyBatis 导航的上游 OpenIDEA 用户可继续使用
0.3.3，该版本仍自带插件侧的行槽箭头与转到定义。

## Known limitation

When the project is open as a *project window group* (workspace), the plugin
API exposes only the group root — its members' files are outside it, so
statement generation and XML creation report that no mapper XML is reachable.
SQL-log conversion works (it reads the editor buffer). Open a member as its
own project, or wait for the host to expose member projects to plugins.

当项目以*项目窗口组*（workspace）方式打开时，插件 API 只暴露组根目录——成员
项目的文件在根之外，因此生成语句与创建 XML 会报告找不到 Mapper XML。SQL
日志转换不受影响（它读取编辑器缓冲区）。请把成员项目单独打开，或等待宿主
向插件暴露成员项目。

## Translations

The plugin ships `locales/zh-CN.json`: the IDE translates the command titles
from it, and the plugin itself translates its notifications and the `@Param`
quickfix title by the UI language (`host::locale()`). Text without an entry
stays English; adding a language is a new `locales/<tag>.json`.

插件自带 `locales/zh-CN.json`：IDE 从中翻译命令标题，插件自身也按 UI 语言
（`host::locale()`）翻译通知与 `@Param` 快速修复标题。没有条目的文案保持
英文；新增语言只需添加 `locales/<tag>.json`。

## Install

Build, then install from disk (Settings › Plugins › Install Plugin from Disk)
or drop the directory into `<project>/.openidea/plugins/` while developing.

构建后从磁盘安装（设置 › 插件 › 从磁盘安装插件），开发时也可把目录放入
`<project>/.openidea/plugins/`。

```sh
rustup target add wasm32-wasip2   # once
cargo build --release --target wasm32-wasip2
mkdir -p dist
cp target/wasm32-wasip2/release/openidea_plugin_mybatis_duo.wasm dist/plugin.wasm
```

The manifest requests `project.read`, `project.write`, `editor.read` and
`clipboard`; the IDE asks for consent on install.

清单声明了 `project.read`、`project.write`、`editor.read` 与 `clipboard`
权限；安装时 IDE 会请求确认。

## License

Apache-2.0. The parsing layer is adapted from the official OpenIDEA MyBatis
Navigator plugin (`plugins/official/mybatis`), which is Apache-2.0.

Apache-2.0。解析层改编自官方 OpenIDEA MyBatis Navigator 插件
（`plugins/official/mybatis`），同为 Apache-2.0。
