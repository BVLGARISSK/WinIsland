//! Proxy choice for the app's own downloads. Proxy apps such as Clash usually only set the
//! Windows system proxy, which reqwest does not read.

const PROXY_VARIABLES: [&str; 4] = ["HTTPS_PROXY", "https_proxy", "ALL_PROXY", "all_proxy"];

/// Routes `builder` through `configured`, or else the system proxy. Proxy environment
/// variables, which reqwest reads itself, win over the system proxy.
pub fn apply(builder: reqwest::ClientBuilder, configured: &str) -> reqwest::ClientBuilder {
    let configured = configured.trim();
    let address = if configured.is_empty() {
        if PROXY_VARIABLES
            .iter()
            .any(|name| std::env::var_os(name).is_some_and(|value| !value.is_empty()))
        {
            return builder;
        }
        match crate::platform::shell().system_proxy() {
            Some(address) => {
                log::info!("Using the system proxy {address}");
                address
            }
            None => return builder,
        }
    } else {
        configured.to_owned()
    };
    match reqwest::Proxy::all(&address) {
        Ok(proxy) => {
            builder.proxy(proxy.no_proxy(reqwest::NoProxy::from_string("localhost,127.0.0.1,::1")))
        }
        Err(error) => {
            log::warn!("Ignoring proxy {address}: {error}");
            builder
        }
    }
}
