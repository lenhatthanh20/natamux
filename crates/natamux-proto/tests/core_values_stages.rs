//! The texts, phases and rules of the stage editor against `packages/core`, which stays the
//! source of truth until phase 4. See `core_values.rs` for the other lists.

// The helpers below are test code too: a source that does not match should fail loudly.
#![allow(clippy::unwrap_used, clippy::panic)]

use natamux_proto::stages::catalog::{
    STAGE_PHASES, StagePhase, catalog_entries, catalog_entry, default_pipeline,
};
use natamux_proto::stages::pipeline_rules::{condition_problems, stage_id_problem};
use natamux_proto::stages::prompt::{PROMPT_PLACEHOLDERS, skill_placeholder};
use natamux_proto::stages::schema::{Condition, GROUP_KINDS, Step};

const STAGE_CATALOG_TS: &str = include_str!("../../../packages/core/src/stages/stage-catalog.ts");
const DEFAULT_PIPELINE_TS: &str =
    include_str!("../../../packages/core/src/stages/default-pipeline.ts");
const PROMPT_TS: &str = include_str!("../../../packages/core/src/stages/prompt.ts");
const SCHEMA_TS: &str = include_str!("../../../packages/core/src/stages/schema.ts");
const PIPELINE_SCHEMA_TS: &str =
    include_str!("../../../packages/core/src/stages/pipeline-schema.ts");
const PIPELINE_ISSUES_TS: &str =
    include_str!("../../../packages/core/src/stages/pipeline-issues.ts");
const TERMINAL_LANE_TS: &str = include_str!("../../../packages/core/src/stages/terminal-lane.ts");
const CAPABILITIES_TS: &str = include_str!("../../../packages/core/src/stages/capabilities.ts");

/// What one `STAGE_CATALOG` entry says in the TypeScript.
struct Written {
    id: String,
    phase: String,
    description: String,
    prompt: Option<String>,
    needs_command: bool,
}

/// The quoted string at the start of `text` (single or double quotes, after spaces and new
/// lines), and what comes after it.
fn quoted(text: &str) -> (String, &str) {
    let text = text.trim_start();
    let quote = text.chars().next().unwrap();
    assert!(quote == '\'' || quote == '"', "not a string: {text:.40}");
    let mut out = String::new();
    let mut chars = text[1..].char_indices();
    while let Some((at, each)) = chars.next() {
        match each {
            '\\' => out.push(chars.next().unwrap().1),
            each if each == quote => return (out, &text[at + 2..]),
            each => out.push(each),
        }
    }
    panic!("a string that never ends");
}

fn constant(source: &str, name: &str) -> String {
    let marker = format!("const {name} = ");
    let start = source.find(&marker).unwrap() + marker.len();
    quoted(&source[start..]).0
}

/// The arguments of `prompt(...)`: strings and the `READ_ONLY` constant.
fn prompt_lines(mut rest: &str, read_only: &str) -> Vec<String> {
    let mut lines = vec!["Task: {title}".to_string()];
    loop {
        rest = rest.trim_start_matches(|c: char| c.is_whitespace() || c == ',');
        if rest.starts_with(')') {
            return lines;
        }
        if let Some(after) = rest.strip_prefix("READ_ONLY") {
            lines.push(read_only.to_string());
            rest = after;
        } else {
            let (line, after) = quoted(rest);
            lines.push(line);
            rest = after;
        }
    }
}

fn written_catalog() -> Vec<Written> {
    let read_only = constant(STAGE_CATALOG_TS, "READ_ONLY");
    let body = &STAGE_CATALOG_TS[STAGE_CATALOG_TS.find("export const STAGE_CATALOG").unwrap()..];
    body.split("    phase: '")
        .skip(1)
        .map(|block| {
            let phase = block[..block.find('\'').unwrap()].to_string();
            let id = quoted(&block[block.find("id: ").unwrap() + 4..]).0;
            let description = block.find("description:").unwrap() + "description:".len();
            let description = quoted(&block[description..]).0;
            let prompt = block.find("prompt: prompt(").map(|at| {
                prompt_lines(&block[at + "prompt: prompt(".len()..], &read_only).join("\n")
            });
            let needs_command = block.contains("needsCommand: true");
            Written {
                id,
                phase,
                description,
                prompt,
                needs_command,
            }
        })
        .collect()
}

fn phase_word(phase: StagePhase) -> &'static str {
    match phase {
        StagePhase::Understand => "understand",
        StagePhase::Design => "design",
        StagePhase::Build => "build",
        StagePhase::Check => "check",
        StagePhase::Ship => "ship",
    }
}

#[test]
fn every_catalog_stage_has_the_phase_description_prompt_and_command_flag_of_core() {
    let written = written_catalog();
    let entries = catalog_entries();
    assert_eq!(written.len(), entries.len(), "the number of catalog stages");
    for (core, entry) in written.iter().zip(entries) {
        let id = &core.id;
        assert_eq!(*id, entry.stage.id, "the order of the catalog");
        assert_eq!(core.phase, phase_word(entry.phase), "{id} phase");
        assert_eq!(core.description, entry.description, "{id} description");
        assert_eq!(core.prompt.as_deref(), entry.prompt, "{id} prompt");
        assert_eq!(core.needs_command, entry.needs_command, "{id} needsCommand");
    }
}

#[test]
fn phases_and_their_names_match_core() {
    let phases = list_of(STAGE_CATALOG_TS, "STAGE_PHASES");
    let rust: Vec<&str> = STAGE_PHASES
        .iter()
        .map(|phase| phase_word(*phase))
        .collect();
    assert_eq!(rust, phases);
    let names = &STAGE_CATALOG_TS[STAGE_CATALOG_TS.find("PHASE_NAMES").unwrap()..];
    for phase in STAGE_PHASES {
        let line = format!("  {}: '{}',", phase_word(phase), phase.name());
        assert!(names.contains(&line), "{line}");
    }
}

/// The quoted strings of `export const NAME ... = [ ... ]`.
fn list_of(source: &str, name: &str) -> Vec<String> {
    let start = source.find(&format!("export const {name}")).unwrap();
    let rest = &source[start..];
    let open = rest.find("= [").unwrap() + 3;
    let close = open + rest[open..].find(']').unwrap();
    rest[open..close]
        .split('\'')
        .skip(1)
        .step_by(2)
        .map(String::from)
        .collect()
}

#[test]
fn prompt_rules_match_core() {
    assert_eq!(
        PROMPT_PLACEHOLDERS.to_vec(),
        list_of(PROMPT_TS, "PROMPT_PLACEHOLDERS")
    );
    assert_eq!(skill_placeholder("x"), "{skill:x}");
    assert!(PROMPT_TS.contains("return `{skill:${name}}`"));
    for text in [
        "Commit your work when it is done.",
        "Do not commit. Leave the changes for review.",
    ] {
        assert!(PROMPT_TS.contains(&format!("'{text}'")), "{text}");
    }
    for line in [
        "'Task: {title}'",
        "`Do the ${stageName(stage)} stage of this task.`",
        "`Write {dir}/${stage.output}.`",
    ] {
        assert!(PROMPT_TS.contains(line), "{line}");
    }
}

#[test]
fn the_limits_and_messages_of_the_rules_match_core() {
    assert!(SCHEMA_TS.contains(
        ".regex(/^[a-z0-9]+(-[a-z0-9]+)*$/, 'A stage id is lower case words joined by \"-\"')"
    ));
    assert!(SCHEMA_TS.contains(".max(40)"));
    assert!(SCHEMA_TS.contains("name: z.string().trim().min(1).max(60).optional()"));
    assert_eq!(
        stage_id_problem("Bad").as_deref(),
        Some("A stage id is lower case words joined by \"-\"")
    );
    assert!(stage_id_problem(&"a".repeat(40)).is_none());
    assert!(stage_id_problem(&"a".repeat(41)).is_some());
    let kinds = list_of(SCHEMA_TS, "GROUP_KINDS");
    let rust: Vec<&str> = GROUP_KINDS.iter().map(|kind| kind.as_str()).collect();
    assert_eq!(rust, kinds);
    let condition = |condition: Condition| condition_problems(&condition);
    let output = condition(Condition::Output {
        file: "x.txt".into(),
        section: None,
        contains: " ".into(),
    });
    let command = condition(Condition::Command { run: " ".into() });
    let texts: Vec<&str> = output
        .iter()
        .chain(&command)
        .map(|(_, message)| message.as_str())
        .collect();
    assert_eq!(
        texts,
        [
            "A condition file is a markdown file, such as plan.md",
            "Give the text to look for",
            "Give the command to run"
        ]
    );
    // Every message of the rules is written in the TypeScript.
    for text in texts.iter().chain(&[
        "Only agent and manual stages can run side by side",
        "An output is a markdown file name, such as spec.md",
    ]) {
        assert!(SCHEMA_TS.contains(text), "{text}");
    }
}

#[test]
fn every_message_of_the_pipeline_rules_is_in_core() {
    for text in [
        "A group holds 2 to 4 stages",
        "A branch holds 2 to 4 paths",
        "A path has at least one stage",
    ] {
        assert!(SCHEMA_TS.contains(text), "{text}");
    }
    for text in [
        "A pipeline has at least one stage",
        "Turn on at least one stage",
        "Only the last path can be the Otherwise path",
        "Turn on a stage in another path, or remove the branch",
        "A group needs 2 stages that are on. Turn on another, or run it on its own",
        "A branch cannot be the first step",
        "A branch cannot come right after a branch. Put a stage between them",
        "The context stage must come first",
        "The release stage must come last",
        "A pipeline has at most one context stage",
        "A pipeline has at most one release stage",
        "which cannot be in a path",
        "Only agent stages can run with a workflow, and",
        "Only a stage that starts an agent or a command can end its terminal, and",
        "Only a stage that changes code or runs tests can be checked on CI, and",
        "Only an agent stage with an output file can run in its own terminal, and",
        "Only an agent or testing stage can have its own agent or arguments, and",
    ] {
        assert!(PIPELINE_SCHEMA_TS.contains(text), "{text}");
    }
    for text in [
        "Give the command to run",
        "A path holds single stages only, not a group or a branch",
        "stages without an output file change code in one worktree at the same time",
    ] {
        assert!(
            PIPELINE_ISSUES_TS.contains(text) || SCHEMA_TS.contains(text),
            "{text}"
        );
    }
    for label in [
        "id: 'Id'",
        "name: 'Name'",
        "prompt: 'Prompt'",
        "output: 'Output file'",
        "sections: 'Required sections'",
        "command: 'Command'",
        "check: 'Check command'",
        "doneWhen: 'Finished when'",
        "agent: 'Agent'",
        "args: 'Arguments'",
    ] {
        assert!(PIPELINE_ISSUES_TS.contains(label), "{label}");
    }
}

#[test]
fn the_terminal_and_capability_rules_match_core() {
    assert!(TERMINAL_LANE_TS.contains("export const OWN_SHELL = '5'"));
    assert!(TERMINAL_LANE_TS.contains("const FIRST_STAGE_SHELL = 6"));
    assert!(CAPABILITIES_TS.contains(
        "stage.kind === 'context' || stage.kind === 'agent' || stage.kind === 'testing'"
    ));
}

#[test]
fn the_default_pipeline_has_the_ids_and_order_of_core() {
    let start = DEFAULT_PIPELINE_TS
        .find("export const DEFAULT_PIPELINE")
        .unwrap();
    let end = DEFAULT_PIPELINE_TS
        .find("export const LEGACY_PIPELINE")
        .unwrap();
    let written = &DEFAULT_PIPELINE_TS[start..end];
    let ids: Vec<&str> = written
        .split("catalogStage('")
        .skip(1)
        .map(|rest| &rest[..rest.find('\'').unwrap()])
        .collect();
    let steps = default_pipeline();
    let rust: Vec<&str> = steps
        .iter()
        .flat_map(|step| step.stages())
        .map(|stage| stage.id.as_str())
        .collect();
    assert_eq!(rust, ids);
    let Step::Group(group) = &steps[3] else {
        panic!("not a group")
    };
    assert!(written.contains(&format!("id: '{}'", group.id)));
    assert!(written.contains(&format!("name: '{}'", group.name.as_deref().unwrap())));
    assert!(
        steps
            .iter()
            .all(|step| catalog_entry(step.id()).is_some() || step.is_group())
    );
}
