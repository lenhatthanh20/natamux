//! `withLoopbackNoProxy` (`no-proxy.ts`).

use crate::pty::Env;

const LOOPBACK_HOSTS: [&str; 3] = ["127.0.0.1", "localhost", "::1"];

/// Claude Code sends its `http` hooks through HTTP_PROXY, and a company proxy refuses a request to
/// loopback, so every hook fails with "proxy refused the connection". Tools read either the upper
/// or the lower case name, so both get the same list.
pub fn with_loopback_no_proxy(env: &Env) -> Env {
    let hosts_of = |name: &str| -> Vec<String> {
        env.get(name)
            .map(|list| {
                list.split(',')
                    .map(str::trim)
                    .filter(|host| !host.is_empty())
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default()
    };
    let mut hosts: Vec<String> = Vec::new();
    let mut all = hosts_of("NO_PROXY");
    all.extend(hosts_of("no_proxy"));
    all.extend(LOOPBACK_HOSTS.iter().map(|host| host.to_string()));
    for host in all {
        if !hosts.iter().any(|known| known.eq_ignore_ascii_case(&host)) {
            hosts.push(host);
        }
    }
    let list = hosts.join(",");
    let mut out = env.clone();
    out.insert("NO_PROXY".into(), list.clone());
    out.insert("no_proxy".into(), list);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(pairs: &[(&str, &str)]) -> Env {
        pairs
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect()
    }

    #[test]
    fn adds_the_loopback_hosts_when_no_list_is_set() {
        let out = with_loopback_no_proxy(&env(&[("PATH", "/bin")]));
        assert_eq!(out["NO_PROXY"], "127.0.0.1,localhost,::1");
        assert_eq!(out["no_proxy"], "127.0.0.1,localhost,::1");
    }

    #[test]
    fn keeps_the_hosts_the_user_set_before_the_loopback_hosts() {
        let out = with_loopback_no_proxy(&env(&[("NO_PROXY", ".corp.example.com")]));
        assert_eq!(out["NO_PROXY"], ".corp.example.com,127.0.0.1,localhost,::1");
        assert_eq!(out["no_proxy"], out["NO_PROXY"]);
    }

    #[test]
    fn uses_the_lower_case_list_when_only_that_one_is_set() {
        let out = with_loopback_no_proxy(&env(&[("no_proxy", "intra.example.com")]));
        assert_eq!(out["NO_PROXY"], "intra.example.com,127.0.0.1,localhost,::1");
        assert_eq!(out["no_proxy"], out["NO_PROXY"]);
    }

    #[test]
    fn joins_both_lists_without_duplicates() {
        let out = with_loopback_no_proxy(&env(&[
            ("NO_PROXY", "a.example.com,b.example.com"),
            ("no_proxy", "b.example.com,c.example.com"),
        ]));
        assert_eq!(
            out["NO_PROXY"],
            "a.example.com,b.example.com,c.example.com,127.0.0.1,localhost,::1"
        );
    }

    #[test]
    fn does_not_add_a_host_that_is_already_there_in_any_case_and_drops_empty_entries() {
        let out = with_loopback_no_proxy(&env(&[("NO_PROXY", " LocalHost , ,127.0.0.1,")]));
        assert_eq!(out["NO_PROXY"], "LocalHost,127.0.0.1,::1");
    }

    #[test]
    fn passes_other_variables_through_and_does_not_change_its_input() {
        let proxy = "http://proxy.example.com:3128";
        let input = env(&[
            ("PATH", "/bin"),
            ("HTTP_PROXY", proxy),
            ("NO_PROXY", "x.example.com"),
        ]);
        let out = with_loopback_no_proxy(&input);
        assert_eq!(out["PATH"], "/bin");
        assert_eq!(out["HTTP_PROXY"], proxy);
        assert_eq!(
            input,
            env(&[
                ("PATH", "/bin"),
                ("HTTP_PROXY", proxy),
                ("NO_PROXY", "x.example.com")
            ])
        );
    }
}
