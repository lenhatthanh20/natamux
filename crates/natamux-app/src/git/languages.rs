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

/// A language of the own highlighter (`text_view/highlight.rs`): how a path finds it, its
/// tree-sitter grammar and the query files that name its parts. The same 22 languages as
/// `LANGUAGES`; the Zed registry above goes once the code view and the diff view use this table.
pub struct Lang {
    /// The name the editor knows it by ("Shell Script").
    pub name: &'static str,
    /// Extensions and whole file names that pick it, compared without case.
    pub suffixes: &'static [&'static str],
    pub grammar: fn() -> tree_sitter::Language,
    /// The query files, in order. On the same text a later pattern wins over an earlier one.
    pub queries: &'static [QueryFile],
    /// Capture names of the vendored files, renamed to the names the theme knows. `None` drops
    /// the capture.
    pub renames: &'static [(&'static str, Option<&'static str>)],
}

/// One query file and its overlap rule.
pub struct QueryFile {
    pub text: &'static str,
    /// The tree-sitter CLI rule of the upstream files: when patterns of this file capture the
    /// same node, the first one wins. Own files use the editor rule (the last one wins).
    pub first_wins: bool,
}

macro_rules! query {
    ($lang:literal, $file:literal, $first_wins:literal) => {
        QueryFile {
            text: include_str!(concat!("../../assets/queries/", $lang, "/", $file, ".scm")),
            first_wins: $first_wins,
        }
    };
    ($lang:literal, $file:literal) => {
        query!($lang, $file, true)
    };
}

// The old editor coloured these upstream files by the editor rule (T-HL-1 shows it), so they
// take it too; so do the CSS and YAML ones below.
const JS: QueryFile = query!("javascript", "highlights", false);
const JSX: QueryFile = query!("javascript", "highlights-jsx", false);
const TS: QueryFile = query!("typescript", "highlights", false);
const JS_EXTRA: QueryFile = query!("javascript", "extra", false);
const TS_EXTRA: QueryFile = query!("typescript", "extra", false);
const JSX_EXTRA: QueryFile = query!("tsx", "extra", false);
const C: QueryFile = query!("c", "highlights");

pub const LANG_COUNT: usize = 22;

/// The languages in the order of `LANGUAGES`. A lookup takes the last one that fits, as the Zed
/// registry does: that is why a `.c` file gets C++ (its suffix `C` fits without case).
pub static LANGS: [Lang; LANG_COUNT] = [
    Lang {
        name: "Shell Script",
        suffixes: &[
            "sh",
            "bash",
            "bashrc",
            "bash_profile",
            "bash_aliases",
            "bash_login",
            "bash_logout",
            "brushrc",
            "bats",
            "envrc",
            "profile",
            "zsh",
            "zshrc",
            "zshenv",
            "zsh_profile",
            "zsh_aliases",
            "zsh_histfile",
            "zlogin",
            "zprofile",
            "PKGBUILD",
            "APKBUILD",
            "ebuild",
        ],
        grammar: || tree_sitter_bash::LANGUAGE.into(),
        queries: &[query!("bash", "highlights"), query!("bash", "extra", false)],
        renames: &[],
    },
    Lang {
        name: "C",
        suffixes: &["c"],
        grammar: || tree_sitter_c::LANGUAGE.into(),
        queries: &[C],
        renames: &[],
    },
    Lang {
        name: "C++",
        suffixes: &[
            "cc", "ccm", "hh", "cpp", "cppm", "h", "hpp", "cxx", "cxxm", "hxx", "c++", "c++m",
            "h++", "hip", "ipp", "inl", "ino", "ixx", "cu", "cuh", "C", "H",
        ],
        grammar: || tree_sitter_cpp::LANGUAGE.into(),
        queries: &[
            C,
            query!("cpp", "highlights"),
            query!("cpp", "extra", false),
        ],
        renames: &[],
    },
    Lang {
        name: "CSS",
        suffixes: &["css", "postcss", "pcss"],
        grammar: || tree_sitter_css::LANGUAGE.into(),
        queries: &[
            query!("css", "highlights", false),
            query!("css", "extra", false),
        ],
        renames: &[],
    },
    Lang {
        name: "Diff",
        suffixes: &["diff", "patch"],
        grammar: || tree_sitter_diff::LANGUAGE.into(),
        queries: &[query!("diff", "highlights")],
        renames: &[("string.special.path", None)],
    },
    Lang {
        name: "Env",
        suffixes: &[".env"],
        grammar: || tree_sitter_bash::LANGUAGE.into(),
        queries: &[query!("env", "highlights", false)],
        renames: &[],
    },
    Lang {
        name: "Git Commit",
        suffixes: &[
            "TAG_EDITMSG",
            "MERGE_MSG",
            "COMMIT_EDITMSG",
            "NOTES_EDITMSG",
            "EDIT_DESCRIPTION",
        ],
        grammar: || tree_sitter_gitcommit::LANGUAGE.into(),
        queries: &[query!("gitcommit", "highlights")],
        // The upstream file uses its own names; these are the theme's.
        renames: &[
            ("branch", Some("string.special.symbol")),
            ("delta", None),
            ("delta.renamed", None),
            ("header", Some("tag")),
            ("minus", Some("diff.minus")),
            ("path", Some("string.special")),
            ("plus", Some("diff.plus")),
            ("trailer.key", Some("variable")),
            ("trailer.value", Some("string")),
        ],
    },
    Lang {
        name: "Go",
        suffixes: &["go"],
        grammar: || tree_sitter_go::LANGUAGE.into(),
        queries: &[query!("go", "highlights"), query!("go", "extra", false)],
        renames: &[],
    },
    Lang {
        name: "Go Mod",
        suffixes: &["mod"],
        grammar: || tree_sitter_gomod::LANGUAGE.into(),
        queries: &[query!("gomod", "highlights")],
        renames: &[],
    },
    Lang {
        name: "Go Work",
        suffixes: &["work"],
        grammar: || tree_sitter_gowork::LANGUAGE.into(),
        queries: &[query!("gowork", "highlights")],
        renames: &[],
    },
    Lang {
        name: "JavaScript",
        suffixes: &["js", "jsx", "mjs", "cjs"],
        grammar: || tree_sitter_typescript::LANGUAGE_TSX.into(),
        queries: &[JS, JSX, JS_EXTRA, JSX_EXTRA],
        renames: &[],
    },
    Lang {
        name: "JSDoc",
        suffixes: &[],
        grammar: || tree_sitter_jsdoc::LANGUAGE.into(),
        queries: &[query!("jsdoc", "highlights")],
        renames: &[],
    },
    Lang {
        name: "JSON",
        suffixes: &[
            "json",
            "flake.lock",
            "geojson",
            "topojson",
            "prettierrc",
            "json.dist",
            "deno.lock",
        ],
        grammar: || tree_sitter_json::LANGUAGE.into(),
        queries: &[query!("json", "highlights"), query!("json", "extra", false)],
        renames: &[
            ("escape", Some("string.escape")),
            ("string.special.key", Some("property")),
        ],
    },
    Lang {
        name: "JSONC",
        suffixes: &[
            "jsonc",
            "bun.lock",
            "devcontainer.json",
            "pyrightconfig.json",
            "tsconfig.json",
            "luaurc",
            "swcrc",
            "babelrc",
            "eslintrc",
            "stylelintrc",
            "jshintrc",
        ],
        grammar: || tree_sitter_json::LANGUAGE.into(),
        queries: &[query!("json", "highlights"), query!("json", "extra", false)],
        renames: &[
            ("escape", Some("string.escape")),
            ("string.special.key", Some("property")),
        ],
    },
    Lang {
        name: "Markdown",
        suffixes: &["md", "mdx", "mdwn", "mdc", "markdown", "MD"],
        grammar: || tree_sitter_md::LANGUAGE.into(),
        queries: &[
            query!("markdown", "highlights"),
            query!("markdown", "extra", false),
        ],
        renames: &[
            ("text.literal", None),
            ("text.reference", Some("link_text")),
            ("text.title", Some("title")),
            ("text.uri", Some("link_uri")),
        ],
    },
    Lang {
        name: "Markdown-Inline",
        suffixes: &[],
        grammar: || tree_sitter_md::INLINE_LANGUAGE.into(),
        queries: &[query!("markdown-inline", "highlights")],
        renames: &[],
    },
    Lang {
        name: "Python",
        suffixes: &["py", "pyi", "mpy"],
        grammar: || tree_sitter_python::LANGUAGE.into(),
        queries: &[
            query!("python", "highlights"),
            query!("python", "extra", false),
        ],
        renames: &[
            ("constant.builtin", None),
            ("constructor", Some("type")),
            ("function.method", Some("function")),
        ],
    },
    Lang {
        name: "Regex",
        suffixes: &[],
        grammar: || tree_sitter_regex::LANGUAGE.into(),
        queries: &[query!("regex", "highlights")],
        renames: &[],
    },
    Lang {
        name: "Rust",
        suffixes: &["rs"],
        grammar: || tree_sitter_rust::LANGUAGE.into(),
        queries: &[query!("rust", "highlights"), query!("rust", "extra", false)],
        renames: &[
            ("attribute", None),
            ("constant.builtin", None),
            ("constructor", Some("type")),
            ("function.method", Some("function")),
            ("label", None),
            ("variable.builtin", Some("variable.special")),
        ],
    },
    Lang {
        name: "TSX",
        suffixes: &["tsx"],
        grammar: || tree_sitter_typescript::LANGUAGE_TSX.into(),
        queries: &[JS, JSX, TS, JS_EXTRA, JSX_EXTRA],
        renames: &[],
    },
    Lang {
        name: "TypeScript",
        suffixes: &["ts", "cts", "mts"],
        grammar: || tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        queries: &[JS, TS, JS_EXTRA, TS_EXTRA],
        renames: &[],
    },
    Lang {
        name: "YAML",
        suffixes: &["yml", "yaml", "pixi.lock", "clang-format", "clangd", "bst"],
        grammar: || tree_sitter_yaml::LANGUAGE.into(),
        queries: &[query!("yaml", "highlights", false)],
        renames: &[("attribute", None), ("label", Some("type"))],
    },
];

/// The language of `path`: its extension (or its whole name when it has no dot) equals a
/// language name or suffix, without case. As in `language_for`, `.env` gives `env`.
#[cfg_attr(not(test), allow(dead_code))]
pub fn lang_for(path: &str) -> Option<&'static Lang> {
    let name = path.rsplit('/').next().unwrap_or(path);
    let key = name.rsplit_once('.').map_or(name, |(_, ext)| ext);
    LANGS.iter().rev().find(|lang| {
        lang.name.eq_ignore_ascii_case(key)
            || lang
                .suffixes
                .iter()
                .any(|suffix| suffix.eq_ignore_ascii_case(key))
    })
}

#[cfg(test)]
#[path = "languages_golden_tests.rs"]
pub(crate) mod golden_tests;
