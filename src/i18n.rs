//! The plugin's own translations. A package's `locales/<tag>.json` maps the
//! English source text to a translation: the IDE translates the manifest and
//! view declarations from those files by itself, and the runtime strings
//! (notifications, quickfix titles, errors) are translated here, keyed by
//! `host::locale()`. Text without an entry stays English.

use openidea_plugin::host;
use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};

const ZH_CN: &str = include_str!("../locales/zh-CN.json");

/// The catalog serving the IDE's UI language, or `None` when the language has
/// no catalog here (English, or an untranslated language). Catalogs are
/// cached per locale, so a language change takes effect without a reload.
fn catalog() -> Option<&'static BTreeMap<String, String>> {
    static CATALOGS: OnceLock<Mutex<BTreeMap<String, Option<&'static BTreeMap<String, String>>>>> = OnceLock::new();
    let locale = host::locale();
    let zh = locale.eq_ignore_ascii_case("zh-CN") || locale.to_ascii_lowercase().starts_with("zh-");
    let tag = if zh { "zh-CN" } else { "en" };
    let cache = CATALOGS.get_or_init(|| Mutex::new(BTreeMap::new()));
    let mut binding = cache.lock().ok()?;
    let entry = binding.entry(tag.to_owned()).or_insert_with(|| {
        if !zh {
            return None;
        }
        let catalog: BTreeMap<String, String> = serde_json::from_str(ZH_CN).ok()?;
        Some(Box::leak(Box::new(catalog)))
    });
    *entry
}

/// `text` in the active language: the catalog's entry, or `text` itself.
pub fn t(text: &str) -> String {
    catalog().and_then(|c| c.get(text).cloned()).unwrap_or_else(|| text.to_owned())
}

/// The translated `text` with `{key}` placeholders replaced by `args` —
/// `{name}` in the source stays literal when `args` has no `name`.
pub fn t_with(text: &str, args: &[(&str, &str)]) -> String {
    let mut out = t(text);
    for (key, value) in args {
        out = out.replace(&format!("{{{key}}}"), value);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    #[test]
    fn translates_when_the_ui_language_is_chinese() {
        let host = MockHost::install();
        host.set_locale("zh-CN");
        assert_eq!(t("The caret is not in a Java file"), "光标不在 Java 文件中");
        assert_eq!(
            t_with("Add @Param annotations to {name}", &[("name", "findNames")]),
            "为 findNames 生成 @Param 注解"
        );
        // An entry-less text stays as written.
        assert_eq!(t("no such entry"), "no such entry");
    }

    #[test]
    fn keeps_english_for_other_locales() {
        let host = MockHost::install();
        host.set_locale("en");
        assert_eq!(t("The caret is not in a Java file"), "The caret is not in a Java file");
    }
}
