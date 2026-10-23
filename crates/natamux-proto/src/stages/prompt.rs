//! The prompt of a stage (`packages/core/src/stages/prompt.ts`).

use crate::stages::capabilities::stage_name;
use crate::stages::catalog::catalog_entry;
use crate::stages::schema::Stage;

/// What a prompt may hold. The daemon fills them when the stage starts.
pub const PROMPT_PLACEHOLDERS: [&str; 4] = ["title", "description", "dir", "ref"];

/// `{skill:<name>}` in a prompt: a skill, called the way the task's agent calls it. A plugin skill
/// is named `<plugin>:<skill>`.
pub fn skill_placeholder(name: &str) -> String {
    format!("{{skill:{name}}}")
}

/// By default you read the changes before they are committed. A stage that checks for a commit
/// has to ask for one, or the agent can never pass it.
pub fn commit_rule(stage: &Stage) -> &'static str {
    if stage.require_commit == Some(true) {
        "Commit your work when it is done."
    } else {
        "Do not commit. Leave the changes for review."
    }
}

/// The prompt of an agent or testing stage when the settings give none. The daemon turns
/// `{commit}` into `commit_rule` of the stage.
pub fn default_prompt(stage: &Stage) -> String {
    // A stage of a project's own may have the id of a catalog stage that came later; it keeps the
    // catalog prompt only when the prompt fits it.
    let entry = catalog_entry(&stage.id);
    let fits = entry.is_some_and(|entry| {
        entry.stage.kind == stage.kind
            && (stage.output.is_none() || stage.output == entry.stage.output)
    });
    if let Some(prompt) = entry.filter(|_| fits).and_then(|entry| entry.prompt) {
        return prompt.to_string();
    }
    let mut lines = vec![
        "Task: {title}".to_string(),
        format!("Do the {} stage of this task.", stage_name(stage)),
    ];
    if let Some(output) = &stage.output {
        lines.push(format!("Write {{dir}}/{output}."));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stages::schema::{Gate, StageKind};

    fn agent(id: &str, output: Option<&str>) -> Stage {
        Stage {
            output: output.map(str::to_owned),
            ..Stage::new(id, StageKind::Agent, Gate::Manual)
        }
    }

    #[test]
    fn asks_the_spec_for_a_code_map() {
        assert!(default_prompt(&agent("spec", None)).contains("Code map"));
    }

    #[test]
    fn takes_the_prompt_of_the_catalog() {
        assert!(default_prompt(&agent("plan", None)).contains("Write {dir}/plan.md"));
        assert!(default_prompt(&agent("security", None)).contains("{dir}/security.md"));
    }

    #[test]
    fn makes_one_for_a_stage_of_its_own() {
        assert_eq!(
            default_prompt(&agent("sign-off", Some("sign-off.md"))),
            "Task: {title}\nDo the Sign off stage of this task.\nWrite {dir}/sign-off.md."
        );
    }

    #[test]
    fn gives_an_older_stage_with_a_catalog_id_its_own_prompt_when_it_does_other_work() {
        assert_eq!(
            default_prompt(&agent("docs", Some("docs.md"))),
            "Task: {title}\nDo the Documentation stage of this task.\nWrite {dir}/docs.md."
        );
    }

    #[test]
    fn names_the_skill_the_commit_rule_and_the_placeholders() {
        assert_eq!(skill_placeholder("validate"), "{skill:validate}");
        assert_eq!(skill_placeholder("plugin:run"), "{skill:plugin:run}");
        let mut stage = agent("implement", None);
        assert_eq!(
            commit_rule(&stage),
            "Do not commit. Leave the changes for review."
        );
        stage.require_commit = Some(true);
        assert_eq!(commit_rule(&stage), "Commit your work when it is done.");
        assert_eq!(PROMPT_PLACEHOLDERS, ["title", "description", "dir", "ref"]);
    }
}
