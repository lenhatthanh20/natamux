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

#[cfg(test)]
mod tests {
    use super::*;

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
