//! `stage-context.ts`: the context stage reads the task's context from its source into the
//! stage file.

use natamux_agents::skill_resolve::{SkillResolution, resolve_skill};
use natamux_proto::projects::repo::ContextSource;
use natamux_proto::stages::schema::Stage;
use natamux_proto::tasks::task::{Task, TaskState};
use natamux_stages::files::{working_dir, write_stage_file};
use natamux_stages::issue::{GhIssue, issue_context, parse_issue_ref};
use natamux_stages::prompts::{context_prompt, manual_context, with_parent_section};
use natamux_store::attachments::{list_attachments, with_attachment_section};
use serde_json::{Map, json};

use crate::stages::errors::{StageError, StageResult, action};
use crate::stages::launcher::Prompt;
use crate::stages::run::{EventInput, StageRun};

fn source(kind: &str, name: Option<&str>, skill: Option<&str>) -> ContextSource {
    ContextSource {
        kind: kind.into(),
        name: name.map(str::to_string),
        skill: skill.map(str::to_string),
        agent: None,
        prompt: None,
    }
}

fn is_agent_source(source: &ContextSource) -> bool {
    source.kind == "skill" || source.kind == "mcp"
}

impl StageRun {
    fn source_of(&self, task: &Task) -> ContextSource {
        let context = task.context.as_ref();
        let kind = context.map_or("manual", |context| context.source.as_str());
        let skill = context.and_then(|context| context.skill.as_deref());
        if let (true, Some(skill)) = (kind == "skill", skill.filter(|skill| !skill.is_empty())) {
            return source("skill", Some(skill), Some(skill));
        }
        let name = context.and_then(|context| context.name.as_deref());
        let offered: Vec<ContextSource> = self
            .settings(task)
            .context
            .sources
            .into_iter()
            .filter(|source| source.kind == kind)
            .collect();
        // A task from before names were sent, or one whose source was renamed, takes the first.
        let configured = offered
            .iter()
            .find(|source| source.name.is_some() && source.name.as_deref() == name)
            .or_else(|| offered.first())
            .cloned();
        configured.unwrap_or_else(|| {
            if kind == "skill" || kind == "mcp" {
                source("manual", None, None)
            } else {
                source(kind, None, None)
            }
        })
    }

    pub fn start_context(&self, task: &Task, stage: &Stage) -> StageResult<()> {
        let source = self.source_of(task);
        if is_agent_source(&source) {
            return self.start_context_agent(task, stage, source);
        }
        self.fetch_context(task, stage, &source)
    }

    /// An agent reads the context with the skill or the MCP server of the source.
    fn start_context_agent(
        &self,
        task: &Task,
        stage: &Stage,
        source: ContextSource,
    ) -> StageResult<()> {
        let attachments = list_attachments(self.data_dir(), &task.id).unwrap_or_default();
        let run = self.weak();
        let owned = task.clone();
        let prompt_for = Box::new(move |agent: &str| {
            let Some(run) = run.upgrade() else {
                return Prompt::default();
            };
            let skill = source
                .skill
                .as_deref()
                .filter(|_| source.kind == "skill")
                .map(|name| resolve_skill(name, &run.skill_context(&owned, agent)));
            let missing = match (&skill, &source.skill) {
                (Some(SkillResolution::Missing), Some(name)) => vec![name.clone()],
                _ => Vec::new(),
            };
            Prompt {
                text: context_prompt(&source, &owned, &attachments, skill.as_ref()),
                missing,
                workflow: false,
            }
        });
        self.start_agent(task, stage, prompt_for, false)
    }

    /// The daemon writes the context of the issue, or of the task itself, then checks it.
    fn fetch_context(&self, task: &Task, stage: &Stage, source: &ContextSource) -> StageResult<()> {
        let mut extra = Map::new();
        extra.insert("source".into(), json!(source.kind));
        if !self.mark_started(task, stage, extra)? {
            return Ok(());
        }
        if let Err(error) = self.write_context(task, stage, source) {
            return self.fail_stage(&task.id, &stage.id, &error.to_string());
        }
        self.record(
            &task.id,
            "context.fetched",
            json!({ "stage": stage.id, "source": source.kind }),
        )?;
        self.report_and_check(&task.id, &stage.id, "system", None)?;
        // A context of the task's own title and description holds nothing new to read. Waiting
        // for approval would put every new backlog task in Needs you.
        let now = self.task(&task.id)?;
        if source.kind == "manual" && now.stage == stage.id && now.state == TaskState::Review {
            let input = EventInput::new("system", "stage.passed", json!({ "stage": stage.id }));
            self.apply_ending(&task.id, input)?;
        }
        Ok(())
    }

    fn write_context(&self, task: &Task, stage: &Stage, source: &ContextSource) -> StageResult<()> {
        let output = stage.output.as_deref().unwrap_or("context.md");
        let own = if source.kind == "github-issue" {
            self.read_issue(task)?
        } else {
            manual_context(task)
        };
        let text = match &task.parent_id {
            None => own,
            Some(parent_id) => {
                let parent = self.task(parent_id)?;
                let folder = working_dir(self.data_dir(), &parent.id, parent.workspace.as_deref());
                with_parent_section(&own, &parent, &folder.to_string_lossy())
            }
        };
        let attachments = list_attachments(self.data_dir(), &task.id).unwrap_or_default();
        // The task may have got its workspace while the issue was read: write where it is now.
        let now = self.task(&task.id)?;
        write_stage_file(
            self.data_dir(),
            &now.id,
            now.workspace.as_deref(),
            output,
            &with_attachment_section(&text, &attachments),
        )
        .map_err(|error| StageError::File(error.to_string()))
    }

    fn read_issue(&self, task: &Task) -> StageResult<String> {
        let reference = task
            .context
            .as_ref()
            .and_then(|context| parse_issue_ref(&context.reference))
            .or_else(|| {
                task.issue
                    .as_ref()
                    .filter(|issue| issue.provider == "github")
                    .and_then(|issue| parse_issue_ref(&issue.key))
            })
            .ok_or_else(|| {
                action("Give the issue number, such as #42, to read the context from")
            })?;
        // An address of another repository reads that repository's issue, not the same number.
        let mut args = vec![
            "issue".to_string(),
            "view".into(),
            reference.number.to_string(),
        ];
        if let Some(repo) = &reference.repo {
            args.extend(["--repo".to_string(), repo.clone()]);
        }
        args.extend([
            "--json".to_string(),
            "number,title,body,url,labels,comments".into(),
        ]);
        let output = (self.options.gh)(&self.repo_path(task), &args)
            .map_err(|error| action(error.message))?;
        let issue = GhIssue::from_json(&output)
            .ok_or_else(|| action("gh gave an answer that is not an issue"))?;
        let linked = self.task(&task.id)?.issue.is_some();
        if let (false, Some(url)) = (linked, issue.url.as_deref().filter(|url| !url.is_empty())) {
            let number = issue.number.unwrap_or(reference.number);
            let key = match &reference.slug {
                Some(slug) => format!("{slug}#{number}"),
                None => format!("#{number}"),
            };
            let link = json!({ "provider": "github", "key": key, "url": url });
            self.options.tasks.link_issue(&task.id, &link, "system")?;
            self.options.bus.deliver_all();
        }
        Ok(issue_context(&issue))
    }

    pub fn is_agent_context(&self, task: &Task) -> bool {
        is_agent_source(&self.source_of(task))
    }
}
