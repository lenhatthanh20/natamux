//! Task templates (`template-service.ts`): `<data>/templates/<id>.md`, the fields in the front
//! matter and the notes of the new task as the body.

use std::cmp::Ordering;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use natamux_core::tasks::inputs::{from_template_input, template, template_input};
use natamux_proto::tasks::task::Task;
use natamux_proto::tasks::template::TaskTemplate;
use natamux_store::front_matter;
use natamux_store::task_files::write_atomic;
use serde_json::{Map, Value, json};

use crate::shared::slug::slugify;
use crate::tasks::errors::{TaskError, TaskResult};
use crate::tasks::service::TaskService;

#[derive(Clone)]
pub struct TemplateService {
    dir: PathBuf,
    tasks: TaskService,
}

/// `/^([a-z0-9-]+)\.md$/`.
fn template_id_of_file(name: &str) -> Option<&str> {
    let id = name.strip_suffix(".md")?;
    let ok = !id.is_empty()
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
    ok.then_some(id)
}

/// `localeCompare` for names. shortcut: case first, then code points; ICU orders accents and
/// punctuation in its own way, so names that differ only there may sort apart from Node's.
fn locale_compare(a: &str, b: &str) -> Ordering {
    a.to_lowercase()
        .cmp(&b.to_lowercase())
        .then_with(|| b.cmp(a))
}

impl TemplateService {
    pub fn new(data_dir: &Path, tasks: TaskService) -> TemplateService {
        TemplateService {
            dir: data_dir.join("templates"),
            tasks,
        }
    }

    pub fn list(&self) -> TaskResult<Vec<TaskTemplate>> {
        let entries = match fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
        };
        let mut templates = Vec::new();
        for entry in entries {
            let name = entry?.file_name().to_string_lossy().into_owned();
            if let Some(id) = template_id_of_file(&name) {
                templates.push(self.get(id)?);
            }
        }
        templates.sort_by(|a, b| locale_compare(&a.name, &b.name));
        Ok(templates)
    }

    pub fn get(&self, id: &str) -> TaskResult<TaskTemplate> {
        let text = match fs::read(self.dir.join(format!("{id}.md"))) {
            Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
            Err(error) if error.kind() == ErrorKind::NotFound => {
                return Err(TaskError::TemplateNotFound(id.into()));
            }
            Err(error) => return Err(error.into()),
        };
        let (data, content) = front_matter::parse(&text).map_err(TaskError::Failed)?;
        let Value::Object(mut data) = data else {
            return Err(TaskError::Failed(format!(
                "The front matter of template {id} is not a mapping"
            )));
        };
        data.insert("id".into(), json!(id));
        data.insert(
            "description".into(),
            json!(natamux_core::js::js_trim(&content)),
        );
        let fields = template(&Value::Object(data))?;
        serde_json::from_value(Value::Object(fields))
            .map_err(|error| TaskError::Failed(error.to_string()))
    }

    /// Saves a template under an id made from its name. A template with the same name is replaced.
    pub fn save(&self, raw: Option<&Value>) -> TaskResult<TaskTemplate> {
        let mut input = template_input(raw)?;
        let name = input
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        input.insert("id".into(), json!(slugify(&name)));
        let fields = template(&Value::Object(input))?;
        let saved: TaskTemplate = serde_json::from_value(Value::Object(fields.clone()))
            .map_err(|error| TaskError::Failed(error.to_string()))?;
        let front: Map<String, Value> = fields
            .into_iter()
            .filter(|(key, _)| key != "id" && key != "description")
            .collect();
        fs::create_dir_all(&self.dir)?;
        let text =
            front_matter::stringify(&format!("{}\n", saved.description), &Value::Object(front));
        write_atomic(&self.dir.join(format!("{}.md", saved.id)), text.as_bytes())?;
        Ok(saved)
    }

    /// A new draft task with the template's notes, priority and labels, and the given title.
    pub fn create_task(&self, id: &str, raw: Option<&Value>) -> TaskResult<Task> {
        let template = self.get(id)?;
        let (title, repo) = from_template_input(raw)?;
        let mut input = json!({ "title": title });
        if let (Some(repo), Some(map)) = (repo, input.as_object_mut()) {
            map.insert("repo".into(), json!(repo));
        }
        if let Some(map) = input.as_object_mut() {
            map.insert("description".into(), json!(template.description));
            map.insert("priority".into(), json!(template.priority));
            map.insert("labels".into(), json!(template.labels));
        }
        self.tasks.create_task(Some(&input), &[], true)
    }
}
