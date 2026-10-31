//! The fake's custom agents: `GET` and `PUT /custom-agents`. It starts with none.

use natamux_proto::agents::custom::CustomAgent;

use super::{Answer, FakeDaemon};

impl FakeDaemon {
    pub fn custom_agents(&self) -> Answer<Vec<CustomAgent>> {
        Ok(self.data().custom_agents.clone())
    }

    pub fn save_custom_agents(&self, list: Vec<CustomAgent>) -> Answer<Vec<CustomAgent>> {
        self.data().custom_agents = list.clone();
        Ok(list)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_with_none_and_keeps_a_saved_list() {
        let fake = FakeDaemon::default();
        assert!(fake.custom_agents().unwrap().is_empty());
        let agent = CustomAgent {
            id: "claude-qwen".into(),
            base: "claude-code".into(),
            command: "cq".into(),
        };
        fake.save_custom_agents(vec![agent.clone()]).unwrap();
        assert_eq!(fake.custom_agents().unwrap(), vec![agent]);
    }
}
