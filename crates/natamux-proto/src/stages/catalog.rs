//! `STAGE_CATALOG` (`packages/core/src/stages/stage-catalog.ts`): the stages a project can pick
//! from, with what a pipeline holds when a stage is added. The long texts are in `catalog_text.rs`.

use crate::stages::catalog_text::TEXTS;
use crate::stages::schema::{Gate, Stage, StageKind, Step};

/// `StagePhase`: the groups of the stage picker.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StagePhase {
    Understand,
    Design,
    Build,
    Check,
    Ship,
}

/// `STAGE_PHASES`.
pub const STAGE_PHASES: [StagePhase; 5] = [
    StagePhase::Understand,
    StagePhase::Design,
    StagePhase::Build,
    StagePhase::Check,
    StagePhase::Ship,
];

impl StagePhase {
    /// `PHASE_NAMES`.
    pub fn name(self) -> &'static str {
        match self {
            StagePhase::Understand => "Understand",
            StagePhase::Design => "Design",
            StagePhase::Build => "Build",
            StagePhase::Check => "Check",
            StagePhase::Ship => "Ship",
        }
    }
}

/// `CatalogStage`. `prompt` is the default prompt of an agent or testing stage; `{commit}` in it
/// becomes the commit rule. `needs_command`: a testing stage that only makes sense with the
/// project's own command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatalogStage {
    pub stage: Stage,
    pub phase: StagePhase,
    pub description: &'static str,
    pub prompt: Option<&'static str>,
    pub needs_command: bool,
}

fn stage(id: &str, kind: StageKind, gate: Gate) -> Stage {
    Stage::new(id, kind, gate)
}

fn named(mut stage: Stage, name: &str) -> Stage {
    stage.name = Some(name.into());
    stage
}

fn writes(mut stage: Stage, output: &str, sections: &[&str]) -> Stage {
    stage.output = Some(output.into());
    if !sections.is_empty() {
        stage.sections = Some(sections.iter().map(|each| (*each).into()).collect());
    }
    stage
}

fn with_workflow(mut stage: Stage) -> Stage {
    stage.workflow = Some(true);
    stage
}

fn with_ci(mut stage: Stage) -> Stage {
    stage.ci = Some(true);
    stage
}

fn with_own_shell(mut stage: Stage) -> Stage {
    stage.own_shell = Some(true);
    stage
}

/// Every entry of the catalog, in the order of `STAGE_CATALOG`, built once.
pub fn catalog_entries() -> &'static [CatalogStage] {
    static ENTRIES: std::sync::OnceLock<Vec<CatalogStage>> = std::sync::OnceLock::new();
    ENTRIES.get_or_init(|| {
        let mut stages = early_stages();
        stages.extend(later_stages());
        // `catalog_text.rs` has the same order; a test checks the ids.
        stages
            .into_iter()
            .zip(TEXTS.iter())
            .map(|(stage, text)| CatalogStage {
                stage,
                phase: text.phase,
                description: text.description,
                prompt: text.prompt,
                needs_command: text.needs_command,
            })
            .collect()
    })
}

/// `catalogEntry(id)`.
pub fn catalog_entry(id: &str) -> Option<&'static CatalogStage> {
    catalog_entries().iter().find(|entry| entry.stage.id == id)
}

/// The stage of every entry, built once: the sidebar names a stage on every row it draws.
pub fn catalog() -> &'static [Stage] {
    static CATALOG: std::sync::OnceLock<Vec<Stage>> = std::sync::OnceLock::new();
    CATALOG.get_or_init(|| {
        catalog_entries()
            .iter()
            .map(|entry| entry.stage.clone())
            .collect()
    })
}

/// The catalog up to Implement: understand, design and build.
fn early_stages() -> Vec<Stage> {
    use Gate::{Auto, Manual};
    use StageKind::{Agent, Context};
    vec![
        writes(stage("context", Context, Manual), "context.md", &[]),
        writes(
            stage("research", Agent, Manual),
            "research.md",
            &["Related files", "How it works today", "Risks"],
        ),
        writes(
            named(stage("clarify", Agent, Manual), "Open questions"),
            "questions.md",
            &["Questions"],
        ),
        writes(
            stage("spec", Agent, Manual),
            "spec.md",
            &[
                "Goal",
                "Scope",
                "Out of scope",
                "Acceptance criteria",
                "Code map",
            ],
        ),
        writes(
            named(stage("ux-design", Agent, Manual), "UX design"),
            "ux.md",
            &["Screens", "States", "Flows", "Accessibility"],
        ),
        writes(
            stage("architecture", Agent, Manual),
            "architecture.md",
            &["Components", "Data flow", "Interfaces", "Trade-offs"],
        ),
        writes(
            stage("threat-model", Agent, Manual),
            "threat-model.md",
            &["Assets", "Entry points", "Threats"],
        ),
        with_workflow(writes(
            stage("plan", Agent, Manual),
            "plan.md",
            &["Steps", "Files", "Tests"],
        )),
        with_own_shell(writes(
            stage("test-spec", Agent, Manual),
            "test-spec.md",
            &["Cases"],
        )),
        with_ci(with_workflow(stage("implement", Agent, Auto))),
    ]
}

/// The catalog from Data migration on: the checks, the reviews and the release.
fn later_stages() -> Vec<Stage> {
    use Gate::{Auto, Manual};
    use StageKind::{Agent, Release, Review, Testing};
    let manual = StageKind::Manual;
    vec![
        named(stage("migration", Agent, Manual), "Data migration"),
        stage("refactor", Agent, Auto),
        named(stage("docs", Agent, Auto), "Documentation"),
        named(stage("i18n", Agent, Auto), "Translations"),
        named(stage("lint", Testing, Auto), "Lint and types"),
        writes(
            named(stage("self-review", Agent, Auto), "Agent self-review"),
            "self-review.md",
            &["Problems"],
        ),
        with_own_shell(writes(
            named(stage("security", Agent, Manual), "Security review"),
            "security.md",
            &["Findings"],
        )),
        with_own_shell(writes(
            named(stage("performance", Agent, Manual), "Performance check"),
            "performance.md",
            &["Findings"],
        )),
        with_own_shell(writes(
            named(stage("accessibility", Agent, Manual), "Accessibility check"),
            "accessibility.md",
            &["Findings"],
        )),
        with_own_shell(writes(
            named(stage("agent-review", Agent, Auto), "Review"),
            "review.md",
            &["Findings"],
        )),
        named(stage("review", Review, Manual), "Manual review"),
        with_ci(named(stage("testing", Testing, Auto), "Automated tests")),
        named(stage("e2e", Testing, Auto), "End-to-end tests"),
        named(stage("qa", manual, Manual), "Manual testing"),
        stage("design-review", manual, Manual),
        named(stage("changelog", Agent, Auto), "Release notes"),
        named(stage("staging", manual, Manual), "Staging check"),
        stage("release", Release, Manual),
    ]
}

/// `catalogEntry(id)?.stage`.
pub fn catalog_stage(id: &str) -> Option<Stage> {
    catalog().iter().find(|stage| stage.id == id).cloned()
}

/// `DEFAULT_PIPELINE`: what Reset to default puts in the editor.
pub fn default_pipeline() -> Vec<Step> {
    use crate::stages::schema::StageGroup;
    let one = |id: &str| catalog_stage(id).unwrap_or_default();
    vec![
        Step::Stage(one("context")),
        Step::Stage(one("spec")),
        Step::Stage(one("plan")),
        Step::Group(StageGroup {
            id: "build".into(),
            name: Some("Test spec and Implement".into()),
            parallel: vec![one("test-spec"), one("implement")],
        }),
        Step::Stage(one("testing")),
        Step::Stage(one("agent-review")),
        Step::Stage(one("release")),
    ]
}

#[cfg(test)]
#[path = "catalog_tests.rs"]
mod tests;
