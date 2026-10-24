//! The `.trim()` of the strings of `projectSettingsSchema` (`core/src/stages/schema.ts`,
//! `core/src/projects/project-settings.ts`): zod hands the daemon each one trimmed, so a branch
//! condition looks for "-" when the file says "- ".

use natamux_core::js::js_trim;
use natamux_proto::projects::repo::ProjectSettings;
use natamux_proto::stages::schema::{Condition, Stage, Step};

fn trim(text: &mut String) {
    let trimmed = js_trim(text);
    if trimmed.len() != text.len() {
        *text = trimmed.to_string();
    }
}

fn trim_some(text: &mut Option<String>) {
    if let Some(text) = text {
        trim(text);
    }
}

fn trim_stage(stage: &mut Stage) {
    for field in [
        &mut stage.name,
        &mut stage.prompt,
        &mut stage.check,
        &mut stage.command,
        &mut stage.done_when,
        &mut stage.agent,
        &mut stage.args,
    ] {
        trim_some(field);
    }
    for section in stage.sections.iter_mut().flatten() {
        trim(section);
    }
}

fn trim_step(step: &mut Step) {
    match step {
        Step::Stage(stage) => trim_stage(stage),
        Step::Group(group) => {
            trim_some(&mut group.name);
            group.parallel.iter_mut().for_each(trim_stage);
        }
        Step::Branch(branch) => {
            trim_some(&mut branch.name);
            for path in &mut branch.paths {
                trim_some(&mut path.name);
                match &mut path.when {
                    Some(Condition::Output {
                        section, contains, ..
                    }) => {
                        trim_some(section);
                        trim(contains);
                    }
                    Some(Condition::Command { run }) => trim(run),
                    None => {}
                }
                path.stages.iter_mut().for_each(trim_stage);
            }
        }
    }
}

pub fn trim_settings(settings: &mut ProjectSettings) {
    settings.stages.iter_mut().for_each(trim_step);
    for source in &mut settings.context.sources {
        trim_some(&mut source.name);
        trim_some(&mut source.skill);
        trim_some(&mut source.prompt);
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn trims_the_strings_zod_trims() {
        let mut settings: ProjectSettings = serde_json::from_value(json!({
            "context": { "sources": [{ "type": "skill", "name": " Jira ", "skill": " jira " }] },
            "stages": [
                { "id": "spec", "kind": "agent", "gate": "manual", "name": " Spec ",
                  "sections": [" Goal "], "check": " make test " },
                { "id": "route", "paths": [
                    { "id": "a", "when": { "type": "output", "file": "spec.md", "section": " Scope ",
                      "contains": "- " }, "stages": [{ "id": "x", "kind": "agent", "gate": "auto" }] },
                    { "id": "b", "when": { "type": "command", "run": " true " },
                      "stages": [{ "id": "y", "kind": "agent", "gate": "auto" }] }
                ] }
            ]
        }))
        .unwrap();
        trim_settings(&mut settings);
        let source = &settings.context.sources[0];
        assert_eq!(
            (source.name.as_deref(), source.skill.as_deref()),
            (Some("Jira"), Some("jira"))
        );
        let Step::Stage(spec) = &settings.stages[0] else {
            panic!("a stage");
        };
        assert_eq!(spec.name.as_deref(), Some("Spec"));
        assert_eq!(spec.sections, Some(vec!["Goal".to_string()]));
        assert_eq!(spec.check.as_deref(), Some("make test"));
        let Step::Branch(branch) = &settings.stages[1] else {
            panic!("a branch");
        };
        assert_eq!(
            branch.paths[0].when,
            Some(Condition::Output {
                file: "spec.md".into(),
                section: Some("Scope".into()),
                contains: "-".into(),
            })
        );
        assert_eq!(
            branch.paths[1].when,
            Some(Condition::Command { run: "true".into() })
        );
    }
}
