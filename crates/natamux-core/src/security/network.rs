//! The `network` key of `security.json`: where Natamux's own git, gh and updater calls may go, a
//! proxy, and whether child processes keep the user's git and GitHub credentials. Read again on
//! each decision, like the mode. Egress is not part of the Strict defaults; `credentials` is.

use std::cell::RefCell;
use std::path::Path;

use serde_json::Value;

use super::source::{FILE_NAME, WRITTEN_NAME};

/// Where Natamux's own calls may go, from open to closed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Egress {
    #[default]
    Open,
    Allowlist,
    Loopback,
}

impl Egress {
    pub fn parse(text: &str) -> Option<Egress> {
        match text {
            "open" => Some(Egress::Open),
            "allowlist" => Some(Egress::Allowlist),
            "loopback" => Some(Egress::Loopback),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Egress::Open => "open",
            Egress::Allowlist => "allowlist",
            Egress::Loopback => "loopback",
        }
    }
}

pub const GIT_ACTIONS: [&str; 4] = ["fetch", "push", "push-force", "delete"];
pub const GH_ACTIONS: [&str; 3] = ["read", "pr-create", "issue-create"];
/// The hosts of a GitHub release download: the page, then the asset store it redirects to.
pub const UPDATE_HOSTS: [&str; 3] = [
    "github.com",
    "objects.githubusercontent.com",
    "release-assets.githubusercontent.com",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitRule {
    pub host: String,
    pub owner: String,
    pub repo: String,
    pub actions: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GhRule {
    pub host: String,
    pub actions: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetworkSettings {
    pub egress: Egress,
    /// Err: a bad entry, so the whole list denies.
    pub git: Result<Vec<GitRule>, String>,
    pub gh: Result<Vec<GhRule>, String>,
    pub updater: bool,
    /// The hosts an update may reach under `allowlist`.
    pub update_hosts: Vec<String>,
    pub proxy: Option<String>,
    pub no_proxy: Vec<String>,
    /// `credentials.agentEnv: strip`.
    pub strip: bool,
    pub strip_extra: Vec<String>,
}

impl NetworkSettings {
    /// The settings with no `network` key: open, credentials kept unless Strict is on.
    pub fn defaults(strict: bool) -> NetworkSettings {
        NetworkSettings {
            egress: Egress::Open,
            git: Ok(Vec::new()),
            gh: Ok(Vec::new()),
            updater: true,
            update_hosts: UPDATE_HOSTS.map(str::to_string).to_vec(),
            proxy: None,
            no_proxy: Vec::new(),
            strip: strict,
            strip_extra: Vec::new(),
        }
    }
}

fn strings(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// A host pattern: a name, or one leading `*.` before a name.
fn host_ok(host: &str) -> bool {
    let name = host.strip_prefix("*.").unwrap_or(host);
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b':'))
}

fn actions_of(entry: &Value, known: &[&str]) -> Option<Vec<String>> {
    let list = entry.get("actions")?.as_array()?;
    let actions: Vec<String> = list
        .iter()
        .map(|action| action.as_str().map(str::to_string))
        .collect::<Option<_>>()?;
    let all_known = actions
        .iter()
        .all(|action| known.contains(&action.as_str()));
    (!actions.is_empty() && all_known).then_some(actions)
}

fn text_or_any(entry: &Value, key: &str) -> Option<String> {
    match entry.get(key) {
        None => Some("*".into()),
        Some(Value::String(text)) if !text.is_empty() && !text.contains('/') => Some(text.clone()),
        Some(_) => None,
    }
}

fn host_of(entry: &Value) -> Option<String> {
    let host = entry.get("host")?.as_str()?.to_ascii_lowercase();
    host_ok(&host).then_some(host)
}

fn git_rule(entry: &Value) -> Option<GitRule> {
    Some(GitRule {
        host: host_of(entry)?,
        owner: text_or_any(entry, "owner")?,
        repo: text_or_any(entry, "repo")?,
        actions: actions_of(entry, &GIT_ACTIONS)?,
    })
}

fn gh_rule(entry: &Value) -> Option<GhRule> {
    Some(GhRule {
        host: host_of(entry)?,
        actions: actions_of(entry, &GH_ACTIONS)?,
    })
}

/// A missing list is empty; a list with one bad entry is an error, so it denies everything.
fn rules<T>(
    value: Option<&Value>,
    key: &str,
    read: fn(&Value) -> Option<T>,
) -> Result<Vec<T>, String> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let list = value
        .as_array()
        .ok_or_else(|| format!("network.{key} is not a list"))?;
    list.iter()
        .enumerate()
        .map(|(at, entry)| {
            read(entry).ok_or_else(|| format!("network.{key} entry {at} is not valid"))
        })
        .collect()
}

/// The `network` object of a `security.json` value. A value that cannot be read fails closed: an
/// unknown egress is `loopback`, an updater that is not `true` is off, an unknown credentials
/// value strips. Under Strict, `inherit` is looser than the default and is ignored.
pub fn network_of(value: &Value, strict: bool) -> NetworkSettings {
    let mut settings = NetworkSettings::defaults(strict);
    let Some(network) = value.get("network") else {
        return settings;
    };
    if let Some(egress) = network.get("egress") {
        settings.egress = egress
            .as_str()
            .and_then(Egress::parse)
            .unwrap_or(Egress::Loopback);
    }
    settings.git = rules(network.get("git"), "git", git_rule);
    settings.gh = rules(network.get("gh"), "gh", gh_rule);
    match network.get("updater") {
        None | Some(Value::Bool(true)) => {}
        Some(Value::Array(_)) => settings.update_hosts = strings(network.get("updater")),
        Some(_) => settings.updater = false,
    }
    let proxy = network.get("proxy");
    settings.proxy = proxy
        .and_then(|proxy| proxy.get("url"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .map(str::to_string);
    settings.no_proxy = strings(proxy.and_then(|proxy| proxy.get("noProxy")));
    let credentials = network.get("credentials");
    settings.strip = match credentials.and_then(|it| it.get("agentEnv")) {
        None => strict,
        Some(Value::String(text)) if text == "inherit" => strict,
        Some(_) => true,
    };
    settings.strip_extra = strings(credentials.and_then(|it| it.get("stripExtra")));
    settings
}

/// The stricter of two settings: the later egress, both strips, the updater only when both allow
/// it, and only the rules both lists have.
pub fn tighten_network(a: &NetworkSettings, b: &NetworkSettings) -> NetworkSettings {
    fn both<T: Clone + PartialEq>(
        a: &Result<Vec<T>, String>,
        b: &Result<Vec<T>, String>,
    ) -> Result<Vec<T>, String> {
        let (a, b) = (
            a.as_ref().map_err(Clone::clone)?,
            b.as_ref().map_err(Clone::clone)?,
        );
        Ok(a.iter().filter(|rule| b.contains(rule)).cloned().collect())
    }
    let mut extra = a.strip_extra.clone();
    extra.extend(
        b.strip_extra
            .iter()
            .filter(|name| !a.strip_extra.contains(name))
            .cloned(),
    );
    NetworkSettings {
        egress: a.egress.max(b.egress),
        git: both(&a.git, &b.git),
        gh: both(&a.gh, &b.gh),
        updater: a.updater && b.updater,
        update_hosts: a
            .update_hosts
            .iter()
            .filter(|host| b.update_hosts.contains(host))
            .cloned()
            .collect(),
        proxy: a.proxy.clone().or_else(|| b.proxy.clone()),
        no_proxy: a.no_proxy.clone(),
        strip: a.strip || b.strip,
        strip_extra: extra,
    }
}

fn file_network(path: &Path, strict: bool) -> Option<NetworkSettings> {
    let text = std::fs::read_to_string(path).ok()?;
    // A broken file counts as Strict with its defaults, as the mode does.
    Some(match serde_json::from_str::<Value>(&text) {
        Ok(value) => network_of(&value, strict),
        Err(_) => NetworkSettings::defaults(true),
    })
}

/// The settings of a data folder. While `security.json` differs from the copy the daemon wrote
/// (an outside change), the stricter of the two counts, as for the mode.
pub fn network_in(data_dir: &Path, strict: bool) -> NetworkSettings {
    let now = file_network(&data_dir.join(FILE_NAME), strict);
    let written = file_network(&data_dir.join(WRITTEN_NAME), strict);
    match (now, written) {
        (None, _) => NetworkSettings::defaults(strict),
        (Some(now), None) => now,
        (Some(now), Some(written)) if now == written => now,
        (Some(now), Some(written)) => tighten_network(&now, &written),
    }
}

thread_local! {
    static FORCED: RefCell<Option<NetworkSettings>> = const { RefCell::new(None) };
}

/// The settings for this decision: a test's forced ones, else the installed data folder's.
/// Machine offline mode (`NATAMUX_OFFLINE`) is always `loopback`.
pub fn network() -> NetworkSettings {
    let mut settings = FORCED
        .with(|forced| forced.borrow().clone())
        .unwrap_or_else(|| {
            let strict = super::mode().is_strict();
            super::installed()
                .and_then(|source| source.path().parent().map(|dir| network_in(dir, strict)))
                .unwrap_or_else(|| NetworkSettings::defaults(strict))
        });
    if crate::offline::offline() {
        settings.egress = Egress::Loopback;
    }
    settings
}

/// `network()` for work in `folder`: its `.natamux.json` (`security.network.egress`) may only
/// move egress toward `loopback`.
pub fn network_for(folder: &Path) -> NetworkSettings {
    let mut settings = network();
    let repo = std::fs::read_to_string(folder.join(".natamux.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok());
    let wanted = repo
        .as_ref()
        .and_then(|repo| repo.pointer("/security/network/egress"))
        .and_then(Value::as_str)
        .and_then(Egress::parse);
    if let Some(wanted) = wanted {
        settings.egress = settings.egress.max(wanted);
    }
    settings
}

/// Test hook: `network()` gives `settings` on this thread until the guard is dropped.
pub fn force_network(settings: NetworkSettings) -> ForcedNetwork {
    let previous = FORCED.with(|cell| cell.replace(Some(settings)));
    ForcedNetwork { previous }
}

#[must_use = "the settings go back when the guard is dropped"]
pub struct ForcedNetwork {
    previous: Option<NetworkSettings>,
}

impl Drop for ForcedNetwork {
    fn drop(&mut self) {
        let previous = self.previous.take();
        FORCED.with(|cell| cell.replace(previous));
    }
}

#[cfg(test)]
#[path = "network_tests.rs"]
mod tests;
