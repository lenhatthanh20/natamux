//! Task templates (`packages/core/src/tasks/template.ts`), as `GET /templates` sends them.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct TaskTemplate {
    pub id: String,
    pub name: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "normal")]
    pub priority: String,
    #[serde(default)]
    pub labels: Vec<String>,
}

fn normal() -> String {
    "normal".into()
}

/// The body of `GET /templates`.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct TemplateList {
    #[serde(default)]
    pub templates: Vec<TaskTemplate>,
}

/// Longest `name` and `title` of a template.
pub const MAX_TEMPLATE_NAME_LENGTH: usize = 100;
pub const MAX_TEMPLATE_TITLE_LENGTH: usize = 200;

/// The body of `POST /templates`. The daemon makes the id from the name.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TemplateInput {
    pub name: String,
    pub title: String,
    pub description: String,
    pub priority: String,
    pub labels: Vec<String>,
}

/// The answer of `POST /templates`.
#[derive(Clone, Debug, Deserialize)]
pub struct TemplateAnswer {
    pub template: TaskTemplate,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sends_a_template_and_reads_the_saved_one() {
        let input = TemplateInput {
            name: "Bug".into(),
            title: "Fix: ".into(),
            description: "Steps".into(),
            priority: "high".into(),
            labels: vec!["bug".into()],
        };
        assert_eq!(
            serde_json::to_string(&input).unwrap(),
            r#"{"name":"Bug","title":"Fix: ","description":"Steps","priority":"high","labels":["bug"]}"#
        );
        let answer: TemplateAnswer =
            serde_json::from_str(r#"{"template":{"id":"bug","name":"Bug","title":"Fix: "}}"#)
                .unwrap();
        assert_eq!(answer.template.id, "bug");
    }

    #[test]
    fn reads_templates_and_fills_defaults() {
        let list: TemplateList = serde_json::from_str(
            r#"{"templates":[
                {"id":"bug","name":"Bug","title":"Fix: ","description":"Steps","priority":"high","labels":["bug"],"x":1},
                {"id":"plain","name":"Plain","title":"T"}
            ]}"#,
        )
        .unwrap();
        assert_eq!(list.templates[0].priority, "high");
        assert_eq!(list.templates[0].labels, ["bug"]);
        assert_eq!(list.templates[1].priority, "normal");
        assert!(list.templates[1].labels.is_empty());
    }
}
