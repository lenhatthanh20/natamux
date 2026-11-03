//! `custom-agents.json` in the data folder: the user's custom agents (Settings > Agents). Only the
//! user's token writes it; a repo cannot, so a cloned repo never names a program to run.

use std::path::Path;

use natamux_agents::custom::set_custom_agents;
use natamux_proto::agents::custom::{CustomAgent, CustomAgentList};

use crate::stages::launcher::write_private;

pub const CUSTOM_AGENTS_FILE: &str = "custom-agents.json";

/// Reads the file into the process list, which starts empty. A missing or broken file leaves it
/// so (a broken one with a log line), and the daemon still starts.
pub fn load_custom_agents(data_dir: &Path) {
    let path = data_dir.join(CUSTOM_AGENTS_FILE);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return;
    };
    match serde_json::from_str::<CustomAgentList>(&text) {
        Ok(list) => set_custom_agents(list.agents),
        Err(error) => eprintln!("{} is not read: {error}", path.display()),
    }
}

/// Writes the checked list (private, then a rename, so a crash leaves the old file whole) and
/// makes it the process list.
pub fn save_custom_agents(data_dir: &Path, agents: Vec<CustomAgent>) -> std::io::Result<()> {
    std::fs::create_dir_all(data_dir)?;
    let list = CustomAgentList { agents };
    let text = serde_json::to_string_pretty(&list).map_err(std::io::Error::other)?;
    let path = data_dir.join(CUSTOM_AGENTS_FILE);
    let mut temporary = path.as_os_str().to_owned();
    temporary.push(".tmp");
    write_private(Path::new(&temporary), &format!("{text}\n"))?;
    std::fs::rename(&temporary, &path)?;
    set_custom_agents(list.agents);
    Ok(())
}
