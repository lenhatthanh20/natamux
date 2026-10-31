//! Syntax colours of the diff: Zed's built-in tree-sitter grammars and their highlight queries
//! (the `grammars` crate), with no language servers. A file takes its language by extension.

use std::sync::Arc;

use futures::FutureExt;
use gpui::{App, Global};
use language::{Language, LanguageRegistry, LoadedLanguage};
use theme::{ActiveTheme, GlobalTheme};

/// The languages of the `grammars` crate that have a grammar here.
const LANGUAGES: [&str; 22] = [
    "bash",
    "c",
    "cpp",
    "css",
    "diff",
    "env",
    "gitcommit",
    "go",
    "gomod",
    "gowork",
    "javascript",
    "jsdoc",
    "json",
    "jsonc",
    "markdown",
    "markdown-inline",
    "python",
    "regex",
    "rust",
    "tsx",
    "typescript",
    "yaml",
];

struct GlobalLanguages(Arc<LanguageRegistry>);

impl Global for GlobalLanguages {}

/// Registers the grammars once, and follows the theme so the colours change with it.
pub fn init(cx: &mut App) {
    let registry = Arc::new(LanguageRegistry::new(cx.background_executor().clone()));
    registry.register_native_grammars(grammars::native_grammars());
    for name in LANGUAGES {
        let config = grammars::load_config(name);
        let loaded = config.clone();
        registry.register_language(
            config.name.clone(),
            config.grammar.clone(),
            config.matcher.clone(),
            config.hidden,
            None,
            Arc::new(move || {
                let config = loaded.clone();
                async move {
                    Ok(LoadedLanguage {
                        config,
                        queries: grammars::load_queries(name),
                        context_provider: None,
                        toolchain_provider: None,
                        manifest_name: None,
                    })
                }
                .boxed()
            }),
        );
    }
    registry.set_theme(cx.theme().clone());
    cx.set_global(GlobalLanguages(registry));
    cx.observe_global::<GlobalTheme>(|cx| {
        let theme = cx.theme().clone();
        if let Some(languages) = cx.try_global::<GlobalLanguages>() {
            languages.0.set_theme(theme);
        }
    })
    .detach();
}

/// The language of `path` by its extension or name, None when no grammar fits.
pub fn language_for(
    path: &str,
    cx: &App,
) -> Option<impl Future<Output = Option<Arc<Language>>> + use<>> {
    let registry = cx.try_global::<GlobalLanguages>()?.0.clone();
    let name = path.rsplit('/').next().unwrap_or(path);
    let key = name
        .rsplit_once('.')
        .map_or(name, |(_, ext)| ext)
        .to_owned();
    Some(async move { registry.language_for_name_or_extension(&key).await.ok() })
}

#[cfg(test)]
#[path = "languages_golden_tests.rs"]
mod golden_tests;
