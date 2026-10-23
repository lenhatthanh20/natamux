use super::*;

#[test]
fn a_path_gets_its_icon() {
    let cases = [
        ("src/app.ts", "typescript"),
        ("src/App.tsx", "react_ts"),
        ("src/app.test.ts", "test-ts"),
        ("src/App.test.tsx", "test-jsx"),
        ("tests/e2e/test.spec.js", "test-js"),
        ("types/env.d.ts", "typescript-def"),
        ("lib.mjs", "javascript"),
        ("package.json", "nodejs"),
        ("packages/web/package.json", "nodejs"),
        ("config.json", "json"),
        ("tsconfig.build.json", "tsconfig"),
        ("vite.config.ts", "vite"),
        ("Dockerfile", "docker"),
        ("docker/prod.Dockerfile", "docker"),
        (".env.local", "tune"),
        ("README.md", "readme"),
        ("docs/plan.md", "markdown"),
        ("scripts/dev.sh", "console"),
        ("logo.PNG", "image"),
        ("Vagrantfile", "vagrant"),
        ("disc.yml", "yaml"),
        ("deploy.sh.bak", "file"),
        (".gitignore", "git"),
        ("Makefile", "makefile"),
        ("no-extension", "file"),
        (".hidden", "file"),
    ];
    for (path, icon) in cases {
        assert_eq!(file_icon_name(path, false), icon, "{path}");
    }
}

#[test]
fn has_every_icon_the_maps_name() {
    let missing: Vec<&str> = named_icons()
        .into_iter()
        .filter(|name| !KNOWN_ICONS.contains(name))
        .collect();
    assert!(missing.is_empty(), "{missing:?}");
}

#[test]
fn takes_the_light_version_when_there_is_one() {
    assert_eq!(file_icon_name("pnpm-workspace.yaml", true), "pnpm_light");
    assert_eq!(file_icon_name("pnpm-workspace.yaml", false), "pnpm");
    assert_eq!(file_icon_name("src/app.ts", true), "typescript");
    for (_, light) in LIGHT_VARIANTS {
        assert!(KNOWN_ICONS.contains(light), "{light}");
    }
}
