//! Conversation access shared by both providers. The shell, hosted search, and
//! host-owned web tools have separate authority; none inherits another's access.
use super::store::{WorkbenchError, WorkbenchResult};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConversationCapabilities {
    pub command_domains: Vec<String>,
    pub browser: bool,
    pub shell_write: bool,
}

impl ConversationCapabilities {
    pub fn resolve(values: &Map<String, Value>, shell_write: bool) -> WorkbenchResult<Self> {
        let text = match values.get("commandDomains") {
            None | Some(Value::Null) => "",
            Some(Value::String(text)) => text,
            _ => return Err(WorkbenchError::invalid("Command sites must be a list of domain names separated by commas")),
        };
        let mut domains = Vec::new();
        for value in text.split([',', '\n']).map(str::trim).filter(|s| !s.is_empty()) {
            let value = value.to_ascii_lowercase();
            let host = value.strip_prefix("*.").unwrap_or(&value);
            if host.len() > 253 || !host.contains('.') || host.split('.').any(|part| {
                part.is_empty() || part.len() > 63 || part.starts_with('-') || part.ends_with('-')
                    || !part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            }) || host.parse::<std::net::IpAddr>().is_ok() {
                return Err(WorkbenchError::invalid("Use public domain names such as example.org or *.example.org, without a scheme, path, or port"));
            }
            if !domains.contains(&value) { domains.push(value); }
        }
        if domains.len() > 64 { return Err(WorkbenchError::invalid("Choose at most 64 command sites")); }
        domains.sort();
        Ok(Self { command_domains: domains, browser: values.get("browser").and_then(Value::as_bool).unwrap_or(false), shell_write })
    }

    /// Thread-local overrides also travel on resume, so a restarted runtime
    /// cannot silently lose the conversation's web or domain restrictions.
    pub fn codex_config(&self, web_search: bool, network: bool, profile: &str) -> Value {
        let mut config = Map::new();
        config.insert("web_search".into(), json!(if web_search { "live" } else { "disabled" }));
        if network {
            config.insert("features.network_proxy".into(), json!(true));
            let domains = if self.command_domains.is_empty() { vec!["*".to_string()] } else { self.command_domains.clone() };
            config.insert(format!("permissions.{profile}.network.domains"), json!(domains.into_iter().map(|host| (host, json!("allow"))).collect::<Map<String, Value>>()));
            config.insert(format!("permissions.{profile}.network.allow_local_binding"), json!(false));
        }
        Value::Object(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_domain_rules_without_accepting_urls_or_ip_literals() {
        for text in ["https://example.com", "example.com:443", "127.0.0.1", "*", "foo..com", "example.com/path", "-bad.com"] {
            assert!(ConversationCapabilities::resolve(json!({"commandDomains":text}).as_object().unwrap(), false).is_err(), "{text}");
        }
        let c = ConversationCapabilities::resolve(json!({"commandDomains":"EXAMPLE.org, *.example.org, example.org", "browser":true}).as_object().unwrap(), true).unwrap();
        assert_eq!(c.command_domains, ["*.example.org", "example.org"]);
        let config = c.codex_config(false, true, "workbench-edit-network");
        assert_eq!(config["web_search"], "disabled");
        assert_eq!(config["features.network_proxy"], true);
        assert_eq!(config["permissions.workbench-edit-network.network.domains"]["example.org"], "allow");
    }
}
