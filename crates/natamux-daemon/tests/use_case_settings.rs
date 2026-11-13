#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use natamux_daemon::projects::settings_parse::parse_text;
use natamux_proto::stages::pipeline_rules::pipeline_issues;

#[test]
fn published_use_case_settings_pass_the_daemon_parser() {
    let page = include_str!("../../../site/src/content/docs/docs/guides/use-cases.mdx");
    let examples: Vec<&str> = page
        .split("```json")
        .skip(1)
        .map(|block| block.split_once('\n').unwrap().1)
        .map(|block| block.split_once("\n```").unwrap().0)
        .collect();
    assert!(examples.len() >= 4, "Publish four complete settings files");
    for (index, example) in examples.iter().enumerate() {
        let parsed = parse_text(example)
            .unwrap_or_else(|error| panic!("Use case settings {}: {error}", index + 1));
        let issues = pipeline_issues(&parsed.settings.stages);
        assert!(issues.is_empty(), "Example {}: {issues:?}", index + 1);
    }
}
