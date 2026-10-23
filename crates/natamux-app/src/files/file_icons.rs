//! The icon name of a file (`lib/file-icons.ts`, `FileIcon.tsx`): whole names first, then name
//! starts, longer endings, then the extension. Names are the ones of Material Icon Theme (MIT).

pub const DEFAULT_ICON: &str = "file";

/// The names the bundled icon set has. A test checks that every name the maps give is here.
#[cfg(test)]
pub const KNOWN_ICONS: &[&str] = &[
    "agent",
    "audio",
    "babel",
    "biome",
    "bun",
    "bun_light",
    "c",
    "certificate",
    "changelog",
    "claude",
    "console",
    "cpp",
    "csharp",
    "css",
    "dart",
    "database",
    "diff",
    "docker",
    "document",
    "editorconfig",
    "elixir",
    "eslint",
    "file",
    "font",
    "gemfile",
    "git",
    "go",
    "go-mod",
    "graphql",
    "h",
    "hpp",
    "html",
    "image",
    "jar",
    "java",
    "javascript",
    "json",
    "jupyter",
    "key",
    "kotlin",
    "less",
    "license",
    "lock",
    "log",
    "lua",
    "makefile",
    "markdown",
    "mdx",
    "nodejs",
    "npm",
    "pdf",
    "php",
    "playwright",
    "pnpm",
    "pnpm_light",
    "powershell",
    "prettier",
    "proto",
    "python",
    "react",
    "react_ts",
    "readme",
    "ruby",
    "rust",
    "sass",
    "settings",
    "svelte",
    "svg",
    "swift",
    "table",
    "terraform",
    "test-js",
    "test-jsx",
    "test-ts",
    "toml",
    "toml_light",
    "tsconfig",
    "tune",
    "typescript",
    "typescript-def",
    "vagrant",
    "video",
    "vite",
    "vitest",
    "vue",
    "webassembly",
    "xml",
    "yaml",
    "yarn",
    "zip",
];

/// Icons that are dark on dark: the set has a lighter one for a light window.
const LIGHT_VARIANTS: &[(&str, &str)] = &[
    ("bun", "bun_light"),
    ("pnpm", "pnpm_light"),
    ("toml", "toml_light"),
];

// Whole file names, lower case. They win over extensions: package.json is not just JSON.
const FILE_NAMES: &[(&str, &str)] = &[
    (".babelrc", "babel"),
    (".dockerignore", "docker"),
    (".editorconfig", "editorconfig"),
    (".env", "tune"),
    (".gitattributes", "git"),
    (".gitignore", "git"),
    (".gitmodules", "git"),
    (".npmrc", "npm"),
    (".nvmrc", "nodejs"),
    (".prettierrc", "prettier"),
    ("agents.md", "agent"),
    ("biome.json", "biome"),
    ("biome.jsonc", "biome"),
    ("bun.lock", "bun"),
    ("bun.lockb", "bun"),
    ("cargo.lock", "lock"),
    ("cargo.toml", "rust"),
    ("changelog.md", "changelog"),
    ("claude.md", "claude"),
    ("compose.yaml", "docker"),
    ("compose.yml", "docker"),
    ("composer.json", "php"),
    ("docker-compose.yaml", "docker"),
    ("docker-compose.yml", "docker"),
    ("dockerfile", "docker"),
    ("gemfile", "gemfile"),
    ("go.mod", "go-mod"),
    ("go.sum", "go-mod"),
    ("license", "license"),
    ("license.md", "license"),
    ("license.txt", "license"),
    ("makefile", "makefile"),
    ("package-lock.json", "npm"),
    ("package.json", "nodejs"),
    ("playwright.config.ts", "playwright"),
    ("pnpm-lock.yaml", "pnpm"),
    ("pnpm-workspace.yaml", "pnpm"),
    ("readme.md", "readme"),
    ("vagrantfile", "vagrant"),
    ("yarn.lock", "yarn"),
];

// File names that start a certain way, such as tsconfig.build.json or vite.config.ts.
const NAME_STARTS: &[(&str, &str)] = &[
    (".env.", "tune"),
    ("dockerfile.", "docker"),
    ("eslint.config.", "eslint"),
    ("tsconfig.", "tsconfig"),
    ("vite.config.", "vite"),
    ("vitest.config.", "vitest"),
];

// Endings longer than one extension come first: a test is not plain TypeScript.
const ENDINGS: &[(&str, &str)] = &[
    (".d.ts", "typescript-def"),
    (".test.ts", "test-ts"),
    (".spec.ts", "test-ts"),
    (".test.tsx", "test-jsx"),
    (".spec.tsx", "test-jsx"),
    (".test.js", "test-js"),
    (".spec.js", "test-js"),
    (".test.mjs", "test-js"),
    (".spec.mjs", "test-js"),
    (".test.jsx", "test-jsx"),
    (".spec.jsx", "test-jsx"),
];

const EXTENSIONS: &[(&str, &str)] = &[
    ("bash", "console"),
    ("bmp", "image"),
    ("c", "c"),
    ("cc", "cpp"),
    ("cfg", "settings"),
    ("cjs", "javascript"),
    ("conf", "settings"),
    ("cpp", "cpp"),
    ("crt", "certificate"),
    ("cs", "csharp"),
    ("css", "css"),
    ("csv", "table"),
    ("cts", "typescript"),
    ("cxx", "cpp"),
    ("dart", "dart"),
    ("diff", "diff"),
    ("dockerfile", "docker"),
    ("env", "tune"),
    ("ex", "elixir"),
    ("exs", "elixir"),
    ("gif", "image"),
    ("go", "go"),
    ("gql", "graphql"),
    ("graphql", "graphql"),
    ("gz", "zip"),
    ("h", "h"),
    ("hpp", "hpp"),
    ("htm", "html"),
    ("html", "html"),
    ("ico", "image"),
    ("ini", "settings"),
    ("ipynb", "jupyter"),
    ("jar", "jar"),
    ("java", "java"),
    ("jpeg", "image"),
    ("jpg", "image"),
    ("js", "javascript"),
    ("json", "json"),
    ("jsonc", "json"),
    ("jsx", "react"),
    ("key", "key"),
    ("kt", "kotlin"),
    ("less", "less"),
    ("lock", "lock"),
    ("log", "log"),
    ("lua", "lua"),
    ("md", "markdown"),
    ("mdx", "mdx"),
    ("mjs", "javascript"),
    ("mov", "video"),
    ("mp3", "audio"),
    ("mp4", "video"),
    ("mts", "typescript"),
    ("otf", "font"),
    ("patch", "diff"),
    ("pdf", "pdf"),
    ("pem", "certificate"),
    ("php", "php"),
    ("phar", "php"),
    ("png", "image"),
    ("proto", "proto"),
    ("ps1", "powershell"),
    ("py", "python"),
    ("rb", "ruby"),
    ("rs", "rust"),
    ("sass", "sass"),
    ("scss", "sass"),
    ("sh", "console"),
    ("sql", "database"),
    ("sqlite", "database"),
    ("svelte", "svelte"),
    ("svg", "svg"),
    ("swift", "swift"),
    ("tar", "zip"),
    ("tf", "terraform"),
    ("tgz", "zip"),
    ("toml", "toml"),
    ("ts", "typescript"),
    ("tsv", "table"),
    ("tsx", "react_ts"),
    ("ttf", "font"),
    ("txt", "document"),
    ("vue", "vue"),
    ("wasm", "webassembly"),
    ("wav", "audio"),
    ("webm", "video"),
    ("webp", "image"),
    ("woff", "font"),
    ("woff2", "font"),
    ("xml", "xml"),
    ("yaml", "yaml"),
    ("yml", "yaml"),
    ("zip", "zip"),
    ("zsh", "console"),
];

fn lookup(table: &[(&'static str, &'static str)], key: &str) -> Option<&'static str> {
    table
        .iter()
        .find(|(name, _)| *name == key)
        .map(|(_, icon)| *icon)
}

/// The icon name of a path. `light` takes the lighter version when the set has one.
pub fn file_icon_name(path: &str, light: bool) -> &'static str {
    let name = path.rsplit('/').next().unwrap_or(path).to_lowercase();
    let icon = base_icon(&name);
    if light {
        lookup(LIGHT_VARIANTS, icon).unwrap_or(icon)
    } else {
        icon
    }
}

fn base_icon(name: &str) -> &'static str {
    if let Some(icon) = lookup(FILE_NAMES, name) {
        return icon;
    }
    if let Some((_, icon)) = NAME_STARTS
        .iter()
        .find(|(start, _)| name.starts_with(start))
    {
        return icon;
    }
    if let Some((_, icon)) = ENDINGS.iter().find(|(ending, _)| name.ends_with(ending)) {
        return icon;
    }
    // A dot at the start is not an extension: .hidden has none.
    match name.rfind('.') {
        Some(dot) if dot > 0 => lookup(EXTENSIONS, &name[dot + 1..]).unwrap_or(DEFAULT_ICON),
        _ => DEFAULT_ICON,
    }
}

/// Every icon name the maps give, sorted, for a check that each one is bundled.
#[cfg(test)]
pub fn named_icons() -> Vec<&'static str> {
    let mut names = vec![DEFAULT_ICON];
    names.extend(FILE_NAMES.iter().map(|(_, icon)| *icon));
    names.extend(NAME_STARTS.iter().map(|(_, icon)| *icon));
    names.extend(ENDINGS.iter().map(|(_, icon)| *icon));
    names.extend(EXTENSIONS.iter().map(|(_, icon)| *icon));
    names.sort_unstable();
    names.dedup();
    names
}

#[cfg(test)]
#[path = "file_icons_tests.rs"]
mod tests;
