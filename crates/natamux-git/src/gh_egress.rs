//! gh calls under the egress setting: offline and `loopback` start no gh; under `allowlist` the
//! call must be one Natamux knows (a read, a pull request or an issue), its host must be in
//! `network.gh`, and gh gets `GH_HOST` (and `GH_REPO` from origin) pinned to what was checked.

use std::path::Path;

use natamux_core::egress::{Remote, Target, enforce, parse_remote};
use natamux_core::security::{Egress, network_for};

use crate::egress::{RemoteOp, resolve_remote};

const FIELDS: [&str; 4] = ["-f", "-F", "--field", "--raw-field"];

fn flag_value<'a>(args: &'a [String], names: &[&str]) -> Vec<&'a str> {
    let mut values = Vec::new();
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        if names.contains(&arg.as_str()) {
            if let Some(value) = rest.next() {
                values.push(value.as_str());
            }
        } else if let Some((name, value)) = arg.split_once('=')
            && names.contains(&name)
        {
            values.push(value);
        } else if let Some(value) = names
            .iter()
            .filter(|name| name.len() == 2)
            .find_map(|name| arg.strip_prefix(name))
        {
            // A short flag with its value attached, such as `-XPOST`.
            values.push(value);
        }
    }
    values
}

/// `gh api` is a read when it uses GET, or when it is a GraphQL `query`.
fn api_is_read(args: &[String]) -> bool {
    if args
        .iter()
        .any(|arg| arg == "--input" || arg.starts_with("--input="))
    {
        return false;
    }
    let fields = flag_value(args, &FIELDS);
    if args.get(1).map(String::as_str) == Some("graphql") {
        let query = fields.iter().find_map(|field| field.strip_prefix("query="));
        return query.is_some_and(|query| {
            let query = query.trim_start();
            (query.starts_with("query") || query.starts_with('{')) && !query.contains("mutation")
        });
    }
    match flag_value(args, &["-X", "--method"]).last() {
        Some(method) => method.eq_ignore_ascii_case("GET"),
        // gh sends a POST as soon as a field is given.
        None => fields.is_empty(),
    }
}

/// The `network.gh` action of a gh call, None for one Natamux does not know.
pub fn classify(args: &[String]) -> Option<&'static str> {
    let first = args.first().map(String::as_str);
    let second = args.get(1).map(String::as_str);
    match (first?, second.unwrap_or("")) {
        ("pr", "create") => Some("pr-create"),
        ("issue", "create") => Some("issue-create"),
        ("pr" | "issue", "view" | "list" | "status" | "checks" | "diff")
        | ("run", "list" | "view" | "watch")
        | ("repo", "view")
        | ("auth", "status") => Some("read"),
        ("api", _) if api_is_read(args) => Some("read"),
        _ => None,
    }
}

// Flags whose value is free text: a URL there names no host.
const TEXT_FLAGS: [&str; 7] = ["-t", "--title", "-b", "--body", "-q", "--jq", "--template"];

/// The host a call names itself: `--hostname`, a `HOST/OWNER/REPO` repo, or a URL argument.
fn named_host(args: &[String]) -> Option<String> {
    if let Some(host) = flag_value(args, &["--hostname"]).last() {
        return Some(host.to_ascii_lowercase());
    }
    let repo = flag_value(args, &["-R", "--repo"]);
    if let Some(repo) = repo.last()
        && repo.split('/').count() == 3
    {
        return repo.split('/').next().map(str::to_ascii_lowercase);
    }
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        if TEXT_FLAGS.contains(&arg.as_str()) {
            rest.next();
            continue;
        }
        let url = match arg.strip_prefix("http://") {
            Some(tail) => format!("https://{tail}"),
            None if arg.starts_with("https://") => arg.clone(),
            None => continue,
        };
        return Some(match parse_remote(&url) {
            Remote::Network { host, .. } => host,
            // An empty host is refused by the egress check, so the call does not fall back to
            // origin's host.
            _ => String::new(),
        });
    }
    None
}

/// The variables that pin gh to the checked host and repo, or why the call is refused. Empty
/// with egress `open`: the old call runs.
pub fn gh_target(cwd: &Path, args: &[String]) -> Result<Vec<(String, String)>, String> {
    if natamux_core::offline::offline() {
        return Err("offline mode: GitHub is not reached".into());
    }
    let settings = network_for(cwd);
    match settings.egress {
        Egress::Open => return Ok(Vec::new()),
        Egress::Loopback => {
            let reason = "egress policy: loopback only, so GitHub is not reached";
            eprintln!("{reason}");
            return Err(reason.into());
        }
        Egress::Allowlist => {}
    }
    let Some(action) = classify(args) else {
        let name = args.first().map_or("", String::as_str);
        let reason = format!("egress policy: gh {name} is not a call Natamux allows");
        eprintln!("{reason}");
        return Err(reason);
    };
    // A host the call names is checked before git reads origin: a refusal starts nothing.
    let named = named_host(args);
    if let Some(host) = &named {
        enforce(&settings, &Target::Gh { host, action })?;
    }
    let origin = match resolve_remote(cwd, RemoteOp::Fetch, "origin").map(|url| parse_remote(&url))
    {
        Ok(Remote::Network {
            host, owner, repo, ..
        }) => Some((host, owner, repo)),
        _ => None,
    };
    let host = match &named {
        Some(host) => host.clone(),
        None => {
            let host = origin
                .as_ref()
                .map_or("github.com", |(host, _, _)| host.as_str());
            enforce(&settings, &Target::Gh { host, action })?;
            host.to_string()
        }
    };
    let mut env = vec![("GH_HOST".to_string(), host.clone())];
    if let Some((origin_host, owner, repo)) = origin
        && origin_host == host
        && !owner.is_empty()
    {
        env.push(("GH_REPO".into(), format!("{host}/{owner}/{repo}")));
    }
    Ok(env)
}

#[cfg(test)]
#[path = "gh_egress_tests.rs"]
mod tests;
