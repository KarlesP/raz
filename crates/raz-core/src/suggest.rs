//! Offline suggestions — no Azure calls.
//!
//! * A recommended **tag set** derived from the Azure Landing Zones resource-tagging policies
//!   (`enforce-resource-group-tags` requires environment/costcenter/owner/workload/sla; the
//!   append/and-values policies add application/department).
//! * CAF-style **resource names** following the Cloud Adoption Framework naming guidance:
//!   `{abbreviation}-{workload}-{env}-{region}-{instance}`.

/// A recommended tag with metadata for display.
pub struct TagSuggestion {
    pub key: &'static str,
    pub required: bool,
    pub example: &'static str,
    pub description: &'static str,
}

/// The recommended tag set. Required tags come from the ALZ `enforce-resource-group-tags`
/// policy; `application`/`department` are recommended additions from the sibling policies.
pub fn recommended_tags() -> &'static [TagSuggestion] {
    &[
        TagSuggestion {
            key: "environment",
            required: true,
            example: "prod",
            description: "Deployment environment (Prod / Dev / Test).",
        },
        TagSuggestion {
            key: "costcenter",
            required: true,
            example: "CC-1234",
            description: "Cost center for chargeback / billing.",
        },
        TagSuggestion {
            key: "owner",
            required: true,
            example: "platform-team",
            description: "Team or person managing the resource.",
        },
        TagSuggestion {
            key: "workload",
            required: true,
            example: "contoso-web",
            description: "Workload or service the resource belongs to.",
        },
        TagSuggestion {
            key: "sla",
            required: true,
            example: "gold",
            description: "Service-level / criticality tier.",
        },
        TagSuggestion {
            key: "application",
            required: false,
            example: "Contoso Web App",
            description: "Application name.",
        },
        TagSuggestion {
            key: "department",
            required: false,
            example: "Finance",
            description: "Owning department.",
        },
    ]
}

/// The name pattern raz suggests (CAF component order).
pub const NAME_PATTERN: &str = "{type}-{workload}-{env}-{region}-{instance}";

/// CAF recommended abbreviations: friendly aliases → abbreviation.
const ABBREVIATIONS: &[(&str, &str)] = &[
    ("resource-group", "rg"),
    ("rg", "rg"),
    ("virtual-machine", "vm"),
    ("vm", "vm"),
    ("virtual-network", "vnet"),
    ("vnet", "vnet"),
    ("subnet", "snet"),
    ("snet", "snet"),
    ("network-interface", "nic"),
    ("nic", "nic"),
    ("public-ip", "pip"),
    ("pip", "pip"),
    ("network-security-group", "nsg"),
    ("nsg", "nsg"),
    ("load-balancer", "lb"),
    ("lb", "lb"),
    ("storage", "st"),
    ("storage-account", "st"),
    ("st", "st"),
    ("key-vault", "kv"),
    ("keyvault", "kv"),
    ("kv", "kv"),
    ("aks", "aks"),
    ("kubernetes", "aks"),
    ("web-app", "app"),
    ("webapp", "app"),
    ("app", "app"),
    ("app-service-plan", "asp"),
    ("asp", "asp"),
    ("function-app", "func"),
    ("func", "func"),
    ("sql-server", "sql"),
    ("sql", "sql"),
    ("sql-database", "sqldb"),
    ("sqldb", "sqldb"),
    ("cosmos", "cosmos"),
    ("cosmosdb", "cosmos"),
    ("log-analytics", "log"),
    ("log", "log"),
    ("managed-identity", "id"),
    ("identity", "id"),
    ("container-registry", "cr"),
    ("acr", "cr"),
    ("cr", "cr"),
];

/// Resolve a resource `kind` (friendly name or abbreviation) to its CAF abbreviation; an
/// unknown kind falls through as its own lowercased prefix.
pub fn abbreviation(kind: &str) -> String {
    let k = kind.to_ascii_lowercase();
    ABBREVIATIONS
        .iter()
        .find(|(name, _)| *name == k)
        .map(|(_, abbr)| (*abbr).to_string())
        .unwrap_or(k)
}

/// Build a CAF-style name `{abbr}-{workload}-{env}-{region}-{instance}`. Globally-restricted
/// types (storage) are lowercased, stripped to alphanumerics, no hyphens, and capped at 24
/// chars; key vault is hyphenated but capped at 24.
pub fn suggest_name(kind: &str, workload: &str, env: &str, region: &str, instance: &str) -> String {
    let abbr = abbreviation(kind);
    let parts = [abbr.as_str(), workload, env, region, instance];

    if abbr == "st" {
        let joined: String = parts
            .iter()
            .flat_map(|p| p.chars())
            .filter(|c| c.is_ascii_alphanumeric())
            .collect::<String>()
            .to_ascii_lowercase();
        return joined.chars().take(24).collect();
    }

    let name = parts
        .iter()
        .filter(|p| !p.is_empty())
        .map(|p| p.to_ascii_lowercase())
        .collect::<Vec<_>>()
        .join("-");

    if abbr == "kv" {
        name.chars().take(24).collect()
    } else {
        name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hyphenated_name() {
        assert_eq!(
            suggest_name("vm", "app", "prod", "weu", "001"),
            "vm-app-prod-weu-001"
        );
        // friendly alias resolves to the abbreviation
        assert_eq!(
            suggest_name("virtual-network", "hub", "prod", "weu", "001"),
            "vnet-hub-prod-weu-001"
        );
    }

    #[test]
    fn storage_has_no_hyphens_lowercase_capped() {
        let n = suggest_name("storage", "App", "Prod", "WEU", "001");
        assert_eq!(n, "stappprodweu001");
        assert!(!n.contains('-'));
        assert!(n.len() <= 24);
    }

    #[test]
    fn unknown_type_falls_through() {
        assert_eq!(
            suggest_name("widget", "app", "prod", "weu", "001"),
            "widget-app-prod-weu-001"
        );
    }

    #[test]
    fn required_tags_present() {
        let keys: Vec<&str> = recommended_tags().iter().map(|t| t.key).collect();
        for req in ["environment", "costcenter", "owner", "workload", "sla"] {
            assert!(keys.contains(&req), "missing required tag {req}");
        }
    }
}
