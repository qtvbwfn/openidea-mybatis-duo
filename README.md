# MyBatis Duo

A plugin for [OpenIDEA](https://github.com/l0ng-ai/openidea) with the whole
MyBatis assist set in one install: navigation both ways (gutter arrows and Go
to Definition between a mapper interface and its XML), statement generation,
completion, the `@Param` quickfix and SQL-log conversion.

Navigation is served from a **background index**: the host feeds documents to
the plugin in cancellable batches and answers arrows and ⌘B from the project
graph, so large projects (1,000+ XML files) never hit the live provider's time
budget.

**Uninstall (or disable) the official MyBatis Navigator** (`openidea.mybatis`)
before installing Duo. The two are separate plugins with separate navigation,
and the host cannot tell that they answer the same targets: with both enabled,
every gutter arrow is drawn twice and ⌘B answers from whichever plugin replies
first. Keep exactly one of them.

OpenIDEA 插件，一次安装提供完整的 MyBatis 辅助功能：双向导航（Mapper 接口与
XML 之间的行槽箭头与转到定义）、生成 Mapper Statement、补全、`@Param` 快速
修复与 SQL 日志转换。

导航由**后台索引**回答：宿主以可取消的批次把文档喂给插件，行槽箭头与 ⌘B 直接
读项目图，因此大项目（1,000+ 个 XML）不会撞上实时 provider 的时间预算。

**安装 Duo 前请先卸载（或停用）官方 MyBatis Navigator**（`openidea.mybatis`）。
两者是各自独立的插件、各有自己的导航，宿主无法判断它们指向同一目标：同时启用
时每个行槽箭头都会画两遍，⌘B 也会由先响应的插件回答。请只保留其中一个。

## Features

- **Navigation both ways** — gutter arrows between a mapper interface and its
  XML statements and back, Go to Definition on namespaces, methods, statement
  ids, `#{…}` parameters, result maps and `<include>` refs, `#{}`/`${}`
  parameter completion, statement-id and `<sql>` refid completion, `@ResultMap`
  ids and annotation-SQL parameters (parsing ported from the official
  `openidea.mybatis` plugin 0.4.1, Apache-2.0). The links wear a blue
  double-arrow badge rather than the IDE's faint default gutter glyph.
- **Statement generation** — for a mapper method with no matching statement,
  create it in the mapper XML (the kind is inferred from the return type and
  refined by the method-name prefix). When the mapper XML does not exist yet,
  `MyBatis Duo: Create Mapper XML` writes the whole file at the Spring
  convention path (`src/main/resources/<package>/<Interface>.xml`).
- **Go to Mapper XML** — `MyBatis Duo: Go to Mapper XML` jumps from the caret:
  a mapper method opens its XML statement (a picker when the name maps to
  several), an XML statement id opens the method. Declared shortcut ⌥⌘B
  (`Ctrl+Alt+B` on Windows/Linux); the IDE owns that key for Go to
  Implementation (the Navigation Bar in the Eclipse keymap), and a plugin
  shortcut never takes a key the IDE already uses — free that binding, or
  assign this command ⌥⌘B, in Settings › Keymap. It is also in Find Action,
  and the gutter badge is a one-click jump.
- **`@Param` quickfix** — a multi-parameter mapper method missing `@Param`
  (or an empty `@Param()`) gets its annotations and import added (⌥↵).
- **Completion** — statement ids from the interface's unmapped methods,
  `<sql>` ids for `<include refid>`, `#{}` parameters of the enclosing
  statement, `@ResultMap` ids (annotation `@Results` + XML `<resultMap>`)
  and parameters inside annotation SQL.
- **SQL-log conversion** — with `==>  Preparing:` / `==> Parameters:` log
  text selected in an editor, copy runnable SQL: the logged type of each
  parameter decides — numbers and booleans stay literal, strings, characters,
  dates and timestamps are quoted and escaped (`00123(String)` → `'00123'`,
  `2024-05-01(Date)` → `'2024-05-01'`; a local date-time's ISO `T` becomes a
  space), and `null` is SQL `NULL` whatever the type — and the statement text is
  kept exactly as logged.

- **双向导航** — Mapper 接口与 XML 语句之间的行槽箭头与转到定义：namespace、
  方法、语句 id、`#{}` 参数、resultMap 与 `<include refid>`；`#{}`/`${}` 参数、
  语句 id 与 `<sql>` refid、`@ResultMap` id 及注解 SQL 参数的补全（解析层移植
  自官方 `openidea.mybatis` 插件 0.4.1，Apache-2.0）。跳转用醒目的蓝色双向
  箭头徽标，而不是 IDE 默认的浅色行槽图标。
- **生成 Mapper Statement** — 为没有对应语句的 Mapper 方法在 Mapper XML 中
  创建语句（类型由返回类型推断，并按方法名前缀修正）。当 Mapper XML 尚不
  存在时，`MyBatis Duo: Create Mapper XML` 会按 Spring 约定路径
  （`src/main/resources/<package>/<Interface>.xml`）写出整个文件。
- **转到 Mapper XML** — `MyBatis Duo: Go to Mapper XML` 从光标处直接跳转：
  Mapper 方法打开对应的 XML 语句（一个名字对应多条时弹选择框），XML 语句 id
  则回到方法。声明的快捷键为 **⌥⌘B**（Windows/Linux 为 `Ctrl+Alt+B`）；该键
  在 IDE 里属于「转到实现」（Eclipse 预设里是导航栏），而插件快捷键不会抢占
  IDE 已用的键，需在 设置 › 按键映射 里释放该绑定或把本命令绑到 ⌥⌘B。命令也在
  「查找操作」里，行槽徽标则是一键直达。
- **`@Param` 快速修复** — 缺少 `@Param`（或 `@Param()` 为空）的多参数 Mapper
  方法会自动补上注解和 import（⌥↵）。
- **补全** — 来自接口未映射方法的语句 id、`<include refid>` 用的 `<sql>` id、
  所在语句的 `#{}` 参数、`@ResultMap` id（注解 `@Results` + XML
  `<resultMap>`）以及注解 SQL 中的参数。
- **SQL 日志转换** — 在编辑器中选中 `==>  Preparing:` / `==> Parameters:`
  日志文本，复制可运行的 SQL：按每个参数的日志类型处理——数字与布尔保持原样，
  字符串、字符、日期与时间戳加引号并转义（`00123(String)` → `'00123'`、
  `2024-05-01(Date)` → `'2024-05-01'`，`LocalDateTime` 的 ISO `T` 换成空格），
  `null` 一律输出 SQL `NULL`（不看类型标注）——语句文本与日志保持完全一致。

## Requirements

OpenIDEA with plugin API 1.5 or newer (v0.1.41+). Navigation needs no plugin
API beyond that: it is the plugin's own index graph, so upstream and fork
builds both get it — and it replaces the official MyBatis Navigator, which
must not stay installed alongside.

需要 OpenIDEA 插件 API 1.5 或更新（v0.1.41+）。导航不依赖额外 API：它由插件
自己的索引图回答，上游与 fork 版本都可用——它替代的是官方 MyBatis Navigator，
请勿与后者同时安装。

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
