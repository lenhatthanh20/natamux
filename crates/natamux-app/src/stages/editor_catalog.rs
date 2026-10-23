//! Adding stages in the pipeline editor (`pipeline-editor/catalog.ts`), and the rules of the stage
//! picker (`StagePicker.tsx`): the stages by phase, the filter, the empty texts, and the form of
//! a stage of your own.
//!
//! `id_from_name` takes accents off the Latin letters it knows (Latin-1, Latin Extended-A and the
//! Vietnamese block) by a table, since the app has no Unicode normalisation crate. Other letters
//! become "-", as in the web.

use natamux_proto::stages::capabilities::{stage_name, stage_name_of_id};
use natamux_proto::stages::catalog::{
    CatalogStage, STAGE_PHASES, StagePhase, catalog_entries, catalog_entry,
};
use natamux_proto::stages::pipeline_rules::{PipelineIssue, stage_id_problem, stage_problems};
use natamux_proto::stages::schema::{Gate, Stage, StageKind, Step};

use crate::stages::editor_branches::update_path;
use crate::stages::editor_draft::{MAX_ID, fits_id_rule};
use crate::stages::editor_find::{
    find_path, insert_at, release_index, stage_fits_in_path, used_ids,
};
use crate::stages::editor_labels::{approval_label, kind_label};

/// Two paths of a branch can break the same rule, and the message does not tell them apart: show
/// it once.
pub fn unique_issues(issues: &[PipelineIssue]) -> Vec<PipelineIssue> {
    let mut seen: Vec<String> = Vec::new();
    let mut kept = Vec::new();
    for issue in issues {
        let key = format!("{:?}|{:?}|{}", issue.level, issue.stage, issue.message);
        if !seen.contains(&key) {
            seen.push(key);
            kept.push(issue.clone());
        }
    }
    kept
}

/// New stages go before the release, which must stay last. A context stage must come first.
fn add_before_release(steps: &[Step], stage: Stage) -> Vec<Step> {
    let at = if stage.kind == StageKind::Context {
        0
    } else {
        release_index(steps)
    };
    insert_at(steps, at, Step::Stage(stage))
}

/// With `path_id`, the stage goes to the end of that path instead. A path takes no context or
/// release stage.
pub fn add_catalog_stage(steps: &[Step], id: &str, path_id: Option<&str>) -> Vec<Step> {
    let Some(entry) = catalog_entry(id) else {
        return steps.to_vec();
    };
    if used_ids(steps).contains(id) {
        return steps.to_vec();
    }
    let stage = entry.stage.clone();
    let Some(path_id) = path_id else {
        return add_before_release(steps, stage);
    };
    if !stage_fits_in_path(&stage) || find_path(steps, path_id).is_none() {
        return steps.to_vec();
    }
    update_path(steps, path_id, |path| path.stages.push(stage.clone()))
}

/// The catalog stages that are not in the pipeline, in catalog order.
pub fn catalog_choices(steps: &[Step]) -> Vec<&'static CatalogStage> {
    let used = used_ids(steps);
    catalog_entries()
        .iter()
        .filter(|entry| !used.contains(&entry.stage.id))
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CustomKind {
    Agent,
    Testing,
    Manual,
}

pub const CUSTOM_KINDS: [CustomKind; 3] =
    [CustomKind::Agent, CustomKind::Testing, CustomKind::Manual];

impl CustomKind {
    pub fn stage_kind(self) -> StageKind {
        match self {
            CustomKind::Agent => StageKind::Agent,
            CustomKind::Testing => StageKind::Testing,
            CustomKind::Manual => StageKind::Manual,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CustomStageInput {
    pub name: String,
    pub id: String,
    pub kind: CustomKind,
    pub output: String,
}

/// The base letter of an accented Latin letter (lower case only).
fn base_letter(letter: char) -> Option<char> {
    const TABLE: [(char, &str); 22] = [
        ('a', "àáâãäåāăąǎǟǡȃȧạảấầẩẫậắằẳẵặḁ"),
        ('c', "çćĉċčḉ"),
        ('d', "ďḋḍḏḑḓ"),
        ('e', "èéêëēĕėęěȅȇȩẹẻẽếềểễệḕḗḙḛḝ"),
        ('g', "ĝğġģǧǵḡ"),
        ('h', "ĥȟḣḥḧḩḫẖ"),
        ('i', "ìíîïĩīĭįǐȉȋỉịḭḯ"),
        ('j', "ĵǰ"),
        ('k', "ķǩḱḳḵ"),
        ('l', "ĺļľḷḹḻḽ"),
        ('m', "ḿṁṃ"),
        ('n', "ñńņňǹṅṇṉṋ"),
        ('o', "òóôõöōŏőơǒǫǭȍȏȫȭȯȱọỏốồổỗộớờởỡợṍṏṑṓ"),
        ('p', "ṕṗ"),
        ('r', "ŕŗřȑȓṙṛṝṟ"),
        ('s', "śŝşšșṡṣṥṧṩ"),
        ('t', "ţťțṫṭṯṱẗ"),
        ('u', "ùúûüũūŭůűųưǔǖǘǚǜȕȗụủứừửữựṳṵṷṹṻ"),
        ('v', "ṽṿ"),
        ('w', "ŵẁẃẅẇẉẘ"),
        ('y', "ýÿŷẏỳỵỷỹẙ"),
        ('z', "źżžẑẓẕ"),
    ];
    TABLE
        .iter()
        .find(|(_, letters)| letters.contains(letter))
        .map(|(base, _)| *base)
}

/// An id from a name: "Load test" becomes "load-test".
pub fn id_from_name(name: &str) -> String {
    let mut id = String::new();
    for letter in name.to_lowercase().chars() {
        if ('\u{300}'..='\u{36f}').contains(&letter) {
            continue;
        }
        let letter = base_letter(letter).unwrap_or(letter);
        if letter.is_ascii_lowercase() || letter.is_ascii_digit() {
            id.push(letter);
        } else if !id.ends_with('-') {
            id.push('-');
        }
    }
    let id = id.trim_matches('-');
    let cut: String = id.chars().take(MAX_ID).collect();
    cut.trim_end_matches('-').to_string()
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CustomStageProblems {
    pub name: Option<String>,
    pub id: Option<String>,
    pub output: Option<String>,
}

impl CustomStageProblems {
    pub fn is_empty(&self) -> bool {
        self.name.is_none() && self.id.is_none() && self.output.is_none()
    }
}

/// The message of the output rule of a stage, by the rules of the proto crate.
fn output_problem(output: &str) -> Option<String> {
    let mut probe = Stage::new("x", StageKind::Agent, Gate::Manual);
    probe.output = Some(output.to_string());
    stage_problems(&probe)
        .into_iter()
        .find(|(field, _)| *field == "output")
        .map(|(_, message)| message)
}

fn id_problem(steps: &[Step], id: &str) -> Option<String> {
    if id.is_empty() {
        return Some("Give the stage an id".to_string());
    }
    if id.chars().count() > MAX_ID {
        return Some(format!("An id has {MAX_ID} characters at most"));
    }
    if !fits_id_rule(id) {
        return Some(
            stage_id_problem(id)
                .unwrap_or_else(|| "A stage id is lower case words joined by \"-\"".to_string()),
        );
    }
    if used_ids(steps).contains(id) {
        return Some("Another stage uses this id".to_string());
    }
    catalog_entry(id).map(|entry| {
        format!(
            "{} is a stage of the list. Add it from there, or pick another id",
            stage_name(&entry.stage)
        )
    })
}

pub fn custom_stage_problems(steps: &[Step], input: &CustomStageInput) -> CustomStageProblems {
    let name = input.name.trim();
    let name_problem = if name.is_empty() {
        Some("Give the stage a name".to_string())
    } else if name.chars().count() > 60 {
        Some("A name has 60 characters at most".to_string())
    } else {
        None
    };
    let output = input.output.trim();
    let output_problem = if input.kind == CustomKind::Agent && !output.is_empty() {
        output_problem(output)
    } else {
        None
    };
    CustomStageProblems {
        name: name_problem,
        id: id_problem(steps, &input.id),
        output: output_problem,
    }
}

/// A stage of your own waits for your approval until you change it.
pub fn add_custom_stage(steps: &[Step], input: &CustomStageInput) -> Vec<Step> {
    if !custom_stage_problems(steps, input).is_empty() {
        return steps.to_vec();
    }
    let name = input.name.trim();
    let output = input.output.trim();
    let mut stage = Stage::new(&input.id, input.kind.stage_kind(), Gate::Manual);
    if name != stage_name_of_id(&input.id) {
        stage.name = Some(name.to_string());
    }
    if input.kind == CustomKind::Agent && !output.is_empty() {
        stage.output = Some(output.to_string());
    }
    add_before_release(steps, stage)
}

pub fn is_catalog_stage(id: &str) -> bool {
    catalog_entry(id).is_some()
}

/// The id of the form: it follows the name until the person edits it.
pub fn form_id(name: &str, own_id: Option<&str>) -> String {
    own_id.map_or_else(|| id_from_name(name), str::to_string)
}

/// The problems the form shows. The id and the output file say what is wrong at once; an empty
/// name waits for the first try to add.
pub fn visible_problems(
    problems: &CustomStageProblems,
    input: &CustomStageInput,
    tried: bool,
) -> CustomStageProblems {
    if tried {
        return problems.clone();
    }
    CustomStageProblems {
        name: None,
        id: if input.id.is_empty() {
            None
        } else {
            problems.id.clone()
        },
        output: if input.output.is_empty() {
            None
        } else {
            problems.output.clone()
        },
    }
}

/// The catalog stages the picker lists, by phase, for what the person typed. A phase with no stage
/// left is left out.
pub fn picker_groups(steps: &[Step], query: &str) -> Vec<(StagePhase, Vec<&'static CatalogStage>)> {
    let words = query.to_lowercase();
    let words = words.trim();
    let shown: Vec<&CatalogStage> = catalog_choices(steps)
        .into_iter()
        .filter(|entry| stage_name(&entry.stage).to_lowercase().contains(words))
        .collect();
    STAGE_PHASES
        .iter()
        .map(|phase| {
            let own = shown.iter().copied().filter(|entry| entry.phase == *phase);
            (*phase, own.collect::<Vec<_>>())
        })
        .filter(|(_, own)| !own.is_empty())
        .collect()
}

/// What the picker says when the list is empty.
pub fn picker_empty_text(steps: &[Step], query: &str) -> Option<String> {
    if !picker_groups(steps, query).is_empty() {
        return None;
    }
    Some(if query.to_lowercase().trim().is_empty() {
        "Every stage of the list is in the pipeline.".to_string()
    } else {
        format!("No stage of the list matches \u{201c}{query}\u{201d}.")
    })
}

/// The line under a stage of the picker: "Agent · Waits for your approval".
pub fn picker_detail(entry: &CatalogStage) -> String {
    format!(
        "{} \u{b7} {}",
        kind_label(entry.stage.kind),
        approval_label(&entry.stage)
    )
}

#[cfg(test)]
#[path = "editor_catalog_tests.rs"]
mod tests;
