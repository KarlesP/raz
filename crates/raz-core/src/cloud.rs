//! Azure cloud (sovereign) definitions. Each cloud has its own ARM/Graph/Vault endpoints and AAD
//! authority; the active one (`raz cloud set`) is threaded through the clients and auth so raz
//! works against AzureUSGovernment / AzureChinaCloud, not just public cloud.

/// Endpoints + authority for one Azure cloud.
pub struct Cloud {
    pub name: &'static str,
    /// ARM endpoint, e.g. `https://management.azure.com`.
    pub arm: &'static str,
    /// AAD authority host, e.g. `https://login.microsoftonline.com`.
    pub authority: &'static str,
    /// Microsoft Graph endpoint, e.g. `https://graph.microsoft.com`.
    pub graph: &'static str,
    /// Key Vault DNS suffix, e.g. `vault.azure.net`.
    pub vault_suffix: &'static str,
}

impl Cloud {
    /// Delegated ARM scope for device-code / refresh-token flows.
    pub fn arm_scope(&self) -> String {
        format!("{}/.default offline_access openid profile", self.arm)
    }
    pub fn graph_scope(&self) -> String {
        format!("{}/.default offline_access openid profile", self.graph)
    }
    pub fn vault_scope(&self) -> String {
        format!(
            "https://{}/.default offline_access openid profile",
            self.vault_suffix
        )
    }
    /// ARM resource for IMDS / managed identity (trailing slash, no scopes).
    pub fn arm_resource(&self) -> String {
        format!("{}/", self.arm)
    }
    /// Microsoft Graph v1.0 base URL.
    pub fn graph_base(&self) -> String {
        format!("{}/v1.0", self.graph)
    }
}

const CLOUDS: &[Cloud] = &[
    Cloud {
        name: "AzureCloud",
        arm: "https://management.azure.com",
        authority: "https://login.microsoftonline.com",
        graph: "https://graph.microsoft.com",
        vault_suffix: "vault.azure.net",
    },
    Cloud {
        name: "AzureUSGovernment",
        arm: "https://management.usgovcloudapi.net",
        authority: "https://login.microsoftonline.us",
        graph: "https://graph.microsoft.us",
        vault_suffix: "vault.usgovcloudapi.net",
    },
    Cloud {
        name: "AzureChinaCloud",
        arm: "https://management.chinacloudapi.cn",
        authority: "https://login.partner.microsoftonline.cn",
        graph: "https://microsoftgraph.chinacloudapi.cn",
        vault_suffix: "vault.azure.cn",
    },
];

/// All known clouds.
pub fn all() -> &'static [Cloud] {
    CLOUDS
}

/// The default cloud (public).
pub fn default() -> &'static Cloud {
    &CLOUDS[0]
}

/// Look up a cloud by name (case-insensitive).
pub fn by_name(name: &str) -> Option<&'static Cloud> {
    CLOUDS.iter().find(|c| c.name.eq_ignore_ascii_case(name))
}

/// Resolve a stored cloud name to a cloud, falling back to the default.
pub fn resolve(name: Option<&str>) -> &'static Cloud {
    name.and_then(by_name).unwrap_or_else(default)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_and_scopes() {
        assert_eq!(resolve(None).name, "AzureCloud");
        assert_eq!(resolve(Some("azureusgovernment")).name, "AzureUSGovernment");
        assert_eq!(resolve(Some("bogus")).name, "AzureCloud");
        assert!(default()
            .arm_scope()
            .starts_with("https://management.azure.com/.default"));
        assert_eq!(default().graph_base(), "https://graph.microsoft.com/v1.0");
    }
}
