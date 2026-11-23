//! TCP of isolated shells, and the `network` part of `GET /security`. Under egress `allowlist`
//! or `loopback` an isolated shell may connect only to the daemon and the proxy (Landlock ABI 4
//! and later); with egress `open` TCP stays open, as the agent needs its model.

use natamux_core::security::{Egress, NetworkSettings};
use natamux_proto::projects::security::NetworkView;
use natamux_pty::Env;

/// The port of an `http(s)://host[:port]/...` address.
fn port_of(url: &str) -> Option<u16> {
    let (scheme, rest) = url.split_once("://")?;
    let authority = rest.split('/').next()?;
    let host_port = authority.rsplit('@').next()?;
    match host_port.rsplit_once(':') {
        Some((host, port)) if !host.ends_with(':') && !port.contains(']') => port.parse().ok(),
        _ => match scheme {
            "https" => Some(443),
            "http" => Some(80),
            _ => None,
        },
    }
}

/// The ports an isolated process may connect to, None when TCP is not restricted.
pub fn tcp_ports(env: &Env, network: &NetworkSettings) -> Option<Vec<u16>> {
    if network.egress == Egress::Open {
        return None;
    }
    let mut ports: Vec<u16> = Vec::new();
    ports.extend(env.get("NATAMUX_URL").and_then(|url| port_of(url)));
    ports.extend(network.proxy.as_deref().and_then(port_of));
    ports.dedup();
    Some(ports)
}

pub const NOT_CONFINED: &str = "child processes not confined";

/// `network` of `GET /security`. `abi`: the Landlock ABI while isolation is on, else None.
pub fn network_view(network: &NetworkSettings, abi: Option<u32>) -> NetworkView {
    let confined = network.egress != Egress::Open && abi.is_some_and(|abi| abi >= 4);
    let note = (!confined).then(|| {
        let why = match (network.egress, abi) {
            (Egress::Open, _) => "egress is open",
            (_, None) => "isolation is off",
            _ => "the kernel's Landlock ABI is below 4",
        };
        format!(
            "{NOT_CONFINED}: {why}. Natamux holds its own git, gh and update calls to the \
             setting; other traffic of agents needs a firewall or proxy."
        )
    });
    NetworkView {
        egress: network.egress.as_str().to_string(),
        updater: network.updater,
        credentials: String::from(if network.strip { "strip" } else { "inherit" }),
        proxy: network.proxy.is_some(),
        confined,
        note,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn allowlist() -> NetworkSettings {
        NetworkSettings {
            egress: Egress::Allowlist,
            proxy: Some("http://proxy.corp:3128".into()),
            ..NetworkSettings::defaults(false)
        }
    }

    #[test]
    fn only_the_daemon_and_the_proxy_ports_under_a_closed_egress() {
        let env: Env = [(
            "NATAMUX_URL".to_string(),
            "http://127.0.0.1:4317".to_string(),
        )]
        .into();
        assert_eq!(tcp_ports(&env, &allowlist()), Some(vec![4317, 3128]));
        assert_eq!(tcp_ports(&env, &NetworkSettings::defaults(true)), None);
        assert_eq!(port_of("https://proxy.corp/"), Some(443));
        assert_eq!(port_of("http://[::1]:9/x"), Some(9));
    }

    #[test]
    fn the_view_says_when_children_are_not_confined() {
        let open = network_view(&NetworkSettings::defaults(true), Some(6));
        assert!(!open.confined && open.credentials == "strip");
        assert!(open.note.unwrap_or_default().starts_with(NOT_CONFINED));
        let old_kernel = network_view(&allowlist(), Some(3));
        assert!(!old_kernel.confined);
        assert!(old_kernel.note.unwrap_or_default().contains("below 4"));
        let confined = network_view(&allowlist(), Some(4));
        assert!(confined.confined && confined.note.is_none() && confined.proxy);
    }
}
