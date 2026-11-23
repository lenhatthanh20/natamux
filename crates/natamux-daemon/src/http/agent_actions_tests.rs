use super::*;

/// Every `(&Method::X, [..])` route pattern of the router files, with each `"a" | "b"` choice
/// and a made-up id for a bound or `_` segment.
fn routes_in(source: &str) -> Vec<(Method, Vec<String>)> {
    let mut found = Vec::new();
    for (start, _) in source.match_indices("(&Method::") {
        let rest = &source[start + "(&Method::".len()..];
        let Some((method, after)) = rest.split_once(',') else {
            continue;
        };
        let after = after.trim_start();
        let (Some(inner), Some(end)) = (after.strip_prefix('['), after.find(']')) else {
            continue;
        };
        let inner = &inner[..end - 1];
        let options: Vec<Vec<String>> = if inner.trim().is_empty() {
            Vec::new()
        } else {
            split_top(inner).iter().map(|each| choices(each)).collect()
        };
        let method = Method::from_bytes(method.trim().as_bytes()).unwrap();
        for parts in product(&options) {
            found.push((method.clone(), parts));
        }
    }
    found
}

fn split_top(text: &str) -> Vec<String> {
    let (mut parts, mut current, mut depth) = (Vec::new(), String::new(), 0);
    for c in text.chars() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(std::mem::take(&mut current));
                continue;
            }
            _ => {}
        }
        current.push(c);
    }
    parts.push(current);
    parts
}

fn choices(element: &str) -> Vec<String> {
    let quoted: Vec<String> = element
        .split('"')
        .skip(1)
        .step_by(2)
        .map(str::to_string)
        .collect();
    if quoted.is_empty() {
        vec!["T-0002".into()]
    } else {
        quoted
    }
}

fn product(options: &[Vec<String>]) -> Vec<Vec<String>> {
    options.iter().fold(vec![Vec::new()], |all, choices| {
        all.iter()
            .flat_map(|start| {
                choices.iter().map(move |choice| {
                    let mut next = start.clone();
                    next.push(choice.clone());
                    next
                })
            })
            .collect()
    })
}

#[test]
fn every_route_of_the_router_is_classified_or_denied_never_both() {
    let mut routes = routes_in(include_str!("server.rs"));
    routes.extend(routes_in(include_str!("routes/workspace_rights.rs")));
    assert!(routes.len() > 80, "the parser found only {}", routes.len());
    for (method, parts) in routes {
        let parts: Vec<&str> = parts.iter().map(String::as_str).collect();
        let classified = classify(&method, &parts).is_some();
        let denied = is_denied(&method, &parts);
        assert!(
            classified != denied,
            "{method} /{}: classified {classified}, denied {denied}",
            parts.join("/")
        );
    }
}

#[test]
fn an_unknown_route_is_neither_classified_nor_listed() {
    for (method, path) in [
        (Method::GET, "nothing"),
        (Method::GET, "tasks/T-1/nothing"),
        (Method::POST, "tasks/T-1/terminals"),
        (Method::DELETE, "events"),
    ] {
        let parts: Vec<&str> = path.split('/').collect();
        assert_eq!(classify(&method, &parts), None, "{method} {path}");
        assert!(!is_denied(&method, &parts), "{method} {path}");
    }
}

#[test]
fn names_the_actions_as_the_strict_grants_do() {
    let of = |method: Method, path: &str| {
        let parts: Vec<&str> = path.split('/').collect();
        classify(&method, &parts)
    };
    use AgentAction::*;
    assert_eq!(of(Method::GET, "tasks"), Some((TasksList, Scope::Global)));
    assert_eq!(
        of(Method::GET, "tasks/T-1"),
        Some((TaskReadOwn, Scope::Child))
    );
    assert_eq!(
        of(Method::GET, "tasks/T-1/tree"),
        Some((FilesRwOwn, Scope::Child))
    );
    assert_eq!(
        of(Method::PUT, "tasks/T-1/file"),
        Some((FilesRwOwn, Scope::Own))
    );
    assert_eq!(
        of(Method::POST, "tasks/T-1/push"),
        Some((GitPushOwn, Scope::Own))
    );
    assert_eq!(
        of(Method::POST, "tasks/T-1/pull-request"),
        Some((PrCreateOwn, Scope::Own))
    );
    assert_eq!(
        of(Method::POST, "tasks/T-1/issue"),
        Some((IssueCreateOwn, Scope::Own))
    );
    assert_eq!(of(Method::GET, "jobs"), Some((GlobalRead, Scope::Global)));
    assert_eq!(
        of(Method::GET, "repos/app/skills"),
        Some((RepoReadOwn, Scope::Global))
    );
    assert_eq!(FilesRwOwn.on_child(), SubtaskReadWorkspace);
    assert_eq!(TaskReadOwn.on_child(), SubtaskReadStatus);
    assert_eq!(GitPushOwn.name(), "git.push.own");
}
