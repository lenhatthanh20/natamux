//! The cases of `template-service.test.ts`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use natamux_daemon::shared::event_bus::EventBus;
use natamux_daemon::tasks::errors::TaskError;
use natamux_daemon::tasks::service::{TaskService, TaskServiceOptions};
use natamux_daemon::tasks::templates::TemplateService;
use natamux_store::task_index::TaskIndex;
use serde_json::{Value, json};

struct Fixture {
    dir: PathBuf,
    tasks: TaskService,
    templates: TemplateService,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.tasks.close();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn fixture(name: &str) -> Fixture {
    let dir = std::env::temp_dir().join(format!("natamux-templates-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let tasks = TaskService::new(TaskServiceOptions {
        data_dir: dir.clone(),
        index: TaskIndex::open(&dir.join("index.sqlite")).unwrap(),
        bus: EventBus::new(),
        now: None,
        settings_of: None,
        clock: None,
    });
    tasks.init().unwrap();
    let templates = TemplateService::new(&dir, tasks.clone());
    Fixture {
        dir,
        tasks,
        templates,
    }
}

fn as_json<T: serde::Serialize>(value: &T) -> Value {
    serde_json::to_value(value).unwrap()
}

#[test]
fn has_no_templates_at_first() {
    let f = fixture("none");
    assert!(f.templates.list().unwrap().is_empty());
}

#[test]
fn saves_a_template_as_a_markdown_file_and_reads_it_back() {
    let f = fixture("save");
    let saved = f
        .templates
        .save(Some(&json!({
            "name": "Bug fix", "title": "Fix", "description": "Find the cause first.",
            "priority": "high", "labels": ["bug"],
        })))
        .unwrap();

    assert_eq!(
        as_json(&saved),
        json!({
            "id": "bug-fix", "name": "Bug fix", "title": "Fix",
            "description": "Find the cause first.", "priority": "high", "labels": ["bug"],
        })
    );
    let text = std::fs::read_to_string(f.dir.join("templates/bug-fix.md")).unwrap();
    assert!(text.contains("Find the cause first."));
    assert_eq!(
        as_json(&f.templates.list().unwrap()),
        json!([as_json(&saved)])
    );
}

#[test]
fn replaces_a_template_with_the_same_name_and_sorts_by_name() {
    let f = fixture("replace");
    f.templates
        .save(Some(&json!({ "name": "Zeta", "title": "Z" })))
        .unwrap();
    f.templates
        .save(Some(&json!({ "name": "Alpha", "title": "A" })))
        .unwrap();
    f.templates
        .save(Some(&json!({ "name": "Alpha", "title": "A2" })))
        .unwrap();

    let listed: Vec<(String, String)> = f
        .templates
        .list()
        .unwrap()
        .into_iter()
        .map(|template| (template.name, template.title))
        .collect();

    assert_eq!(
        listed,
        [
            ("Alpha".to_string(), "A2".to_string()),
            ("Zeta".to_string(), "Z".to_string())
        ]
    );
}

#[test]
fn makes_a_draft_task_from_a_template_with_a_new_title() {
    let f = fixture("draft");
    f.templates
        .save(Some(&json!({ "name": "Bug fix", "title": "Fix", "description": "Steps", "labels": ["bug"] })))
        .unwrap();

    let task = f
        .templates
        .create_task("bug-fix", Some(&json!({ "title": "Fix the login" })))
        .unwrap();

    assert_eq!(task.title, "Fix the login");
    assert_eq!(task.description, "Steps");
    assert_eq!(task.labels, ["bug"]);
    assert_eq!(task.priority, "normal");
    assert_eq!(as_json(&task.state), "draft");
    assert_eq!(task.repo, None);
}

#[test]
fn takes_the_id_from_the_start_of_a_title_and_gives_a_plain_title_the_next_id() {
    let f = fixture("ids");
    f.templates
        .save(Some(&json!({ "name": "Bug fix", "title": "Fix" })))
        .unwrap();

    let named = f
        .templates
        .create_task("bug-fix", Some(&json!({ "title": "abc-3: Fix it" })))
        .unwrap();
    assert_eq!(
        (named.id.as_str(), named.title.as_str()),
        ("ABC-3", "Fix it")
    );
    let plain = f
        .templates
        .create_task("bug-fix", Some(&json!({ "title": "Fix more" })))
        .unwrap();
    assert_eq!(plain.id, "T-0001");
}

#[test]
fn puts_a_task_from_a_template_in_the_repository_it_is_given() {
    let f = fixture("repo");
    f.templates
        .save(Some(&json!({ "name": "Bug fix", "title": "Fix" })))
        .unwrap();

    let task = f
        .templates
        .create_task(
            "bug-fix",
            Some(&json!({ "title": "Fix it", "repo": "web" })),
        )
        .unwrap();

    assert_eq!(task.repo.as_deref(), Some("web"));
}

#[test]
fn refuses_an_unknown_template_and_an_empty_title() {
    let f = fixture("refuse");
    let unknown = f
        .templates
        .create_task("nope", Some(&json!({ "title": "x" })));
    assert!(matches!(unknown, Err(TaskError::TemplateNotFound(_))));
    f.templates
        .save(Some(&json!({ "name": "Bug fix", "title": "Fix" })))
        .unwrap();
    assert!(
        f.templates
            .create_task("bug-fix", Some(&json!({ "title": " " })))
            .is_err()
    );
}
