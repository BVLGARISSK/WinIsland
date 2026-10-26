use windows_registry::CURRENT_USER;

const INTERNET_SETTINGS: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings";

pub(super) fn system() -> Option<String> {
    let key = CURRENT_USER.open(INTERNET_SETTINGS).ok()?;
    if key.get_u32("ProxyEnable").ok()? == 0 {
        return None;
    }
    parse(&key.get_string("ProxyServer").ok()?)
}

/// `host:port` for every protocol, or `http=host:port;https=host:port;...`.
fn parse(server: &str) -> Option<String> {
    let server = server.trim();
    let address = if server.contains('=') {
        let entries: Vec<(&str, &str)> = server
            .split(';')
            .filter_map(|entry| entry.split_once('='))
            .collect();
        ["https", "http"].iter().find_map(|scheme| {
            entries
                .iter()
                .find(|(key, _)| key.trim().eq_ignore_ascii_case(scheme))
                .map(|(_, value)| value.trim())
        })?
    } else {
        server
    };
    if address.is_empty() {
        None
    } else if address.contains("://") {
        Some(address.to_owned())
    } else {
        Some(format!("http://{address}"))
    }
}
