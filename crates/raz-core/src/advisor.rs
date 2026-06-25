//! Architecture advisor — deterministic analysis of a subscription's resources against CAF naming
//! and the ALZ tag set, plus governance posture, producing a profile + rule-based guardrails. This
//! is the model-free backbone; the optional GGUF advisor (Phase 2) turns the profile into prose.

use serde::Serialize;
use serde_json::Value;

use crate::suggest;

#[derive(Serialize)]
pub struct Count {
    pub name: String,
    pub count: usize,
}

#[derive(Serialize)]
pub struct Naming {
    pub conforming: usize,
    pub total: usize,
    pub percent: u8,
    pub violations: Vec<String>,
}

#[derive(Serialize)]
pub struct TagCoverage {
    pub tag: String,
    pub covered: usize,
    pub total: usize,
    pub percent: u8,
}

#[derive(Serialize)]
pub struct Governance {
    pub assignments: usize,
    pub non_compliant: i64,
}

#[derive(Serialize)]
pub struct Profile {
    pub subscription: String,
    pub resource_count: usize,
    pub resource_groups: usize,
    pub services: Vec<Count>,
    pub regions: Vec<Count>,
    pub naming: Naming,
    pub tagging: Vec<TagCoverage>,
    pub governance: Governance,
    pub signals: Vec<String>,
    pub warnings: Vec<String>,
}

/// Map an ARM resource type to its CAF abbreviation (kind), if known.
pub fn arm_type_to_kind(arm_type: &str) -> Option<&'static str> {
    let last = arm_type
        .rsplit('/')
        .next()
        .unwrap_or(arm_type)
        .to_ascii_lowercase();
    Some(match last.as_str() {
        "virtualmachines" => "vm",
        "virtualnetworks" => "vnet",
        "networksecuritygroups" => "nsg",
        "publicipaddresses" => "pip",
        "networkinterfaces" => "nic",
        "storageaccounts" => "st",
        "vaults" => "kv",
        "managedclusters" => "aks",
        "sites" => "app",
        "serverfarms" => "asp",
        "disks" => "disk",
        "loadbalancers" => "lb",
        "routetables" => "rt",
        _ => return None,
    })
}

fn pct(part: usize, whole: usize) -> u8 {
    match (part * 100).checked_div(whole) {
        Some(p) => p as u8,
        None => 100, // empty set: treat as fully covered
    }
}

fn str_field<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

/// Group resources by a string field into descending-count rows.
fn counts_by(resources: &[Value], key: &str) -> Vec<Count> {
    let mut map: Vec<(String, usize)> = Vec::new();
    for r in resources {
        let val = str_field(r, key);
        if val.is_empty() {
            continue;
        }
        match map.iter_mut().find(|(k, _)| k == val) {
            Some((_, c)) => *c += 1,
            None => map.push((val.to_string(), 1)),
        }
    }
    map.sort_by_key(|(_, c)| std::cmp::Reverse(*c));
    map.into_iter()
        .map(|(name, count)| Count { name, count })
        .collect()
}

/// Does `name` follow the CAF abbreviation for `kind`? Hyphen types want `kind-…`; no-hyphen types
/// (storage) just the prefix.
fn name_conforms(name: &str, kind: &str) -> bool {
    let name = name.to_ascii_lowercase();
    if kind == "st" {
        name.starts_with("st")
    } else {
        name.starts_with(&format!("{kind}-"))
    }
}

/// Analyze the gathered subscription data into an [`Profile`].
pub fn analyze(
    resources: &Value,
    resource_groups: &Value,
    governance: &Value,
    subscription: &str,
) -> Profile {
    let res: Vec<Value> = resources.as_array().cloned().unwrap_or_default();
    let rg_count = resource_groups.as_array().map(|a| a.len()).unwrap_or(0);

    let services = counts_by(&res, "type");
    let regions = counts_by(&res, "location");

    // Naming conformance over resources whose type we recognize.
    let mut conforming = 0;
    let mut typed = 0;
    let mut violations = Vec::new();
    for r in &res {
        let Some(kind) = arm_type_to_kind(str_field(r, "type")) else {
            continue;
        };
        typed += 1;
        let name = str_field(r, "name");
        if name_conforms(name, kind) {
            conforming += 1;
        } else if violations.len() < 5 {
            violations.push(format!("{name} (expected `{kind}-…`)"));
        }
    }
    let naming = Naming {
        conforming,
        total: typed,
        percent: pct(conforming, typed),
        violations,
    };

    // Required-tag coverage.
    let total = res.len();
    let tagging: Vec<TagCoverage> = suggest::recommended_tags()
        .iter()
        .filter(|t| t.required)
        .map(|t| {
            let covered = res
                .iter()
                .filter(|r| {
                    r.get("tags")
                        .and_then(Value::as_object)
                        .map(|tags| tags.keys().any(|k| k.eq_ignore_ascii_case(t.key)))
                        .unwrap_or(false)
                })
                .count();
            TagCoverage {
                tag: t.key.to_string(),
                covered,
                total,
                percent: pct(covered, total),
            }
        })
        .collect();

    // Governance from the policy scan.
    let gov_rows = governance.as_array().cloned().unwrap_or_default();
    let non_compliant: i64 = gov_rows
        .iter()
        .filter_map(|r| r.get("nonCompliant").and_then(Value::as_i64))
        .sum();
    let governance = Governance {
        assignments: gov_rows.len(),
        non_compliant,
    };

    // Detected signals.
    let mut signals = Vec::new();
    let envs: Vec<&str> = ["prod", "dev", "test", "staging", "uat"]
        .into_iter()
        .filter(|e| {
            res.iter().any(|r| {
                str_field(r, "name").to_ascii_lowercase().contains(e)
                    || r.get("tags")
                        .and_then(Value::as_object)
                        .map(|t| {
                            t.values()
                                .any(|v| v.as_str().unwrap_or("").to_ascii_lowercase().contains(e))
                        })
                        .unwrap_or(false)
            })
        })
        .collect();
    if !envs.is_empty() {
        signals.push(format!("Environments detected: {}", envs.join(", ")));
    }
    let vnets = services
        .iter()
        .find(|c| c.name.ends_with("virtualNetworks"))
        .map(|c| c.count)
        .unwrap_or(0);
    if vnets > 1 {
        signals.push(format!(
            "{vnets} virtual networks (possible hub-spoke topology)"
        ));
    }
    let has_pip = services
        .iter()
        .any(|c| c.name.ends_with("publicIPAddresses"));
    if has_pip {
        signals.push("Public IP address(es) present — internet exposure".to_string());
    }

    // Rule-based guardrails.
    let mut warnings = Vec::new();
    if naming.total > 0 && naming.percent < 75 {
        warnings.push(format!(
            "Naming: only {}% of resources follow CAF abbreviations",
            naming.percent
        ));
    }
    for t in &tagging {
        if t.percent < 80 {
            warnings.push(format!(
                "Tag '{}': only {}% coverage (required)",
                t.tag, t.percent
            ));
        }
    }
    if governance.assignments == 0 {
        warnings.push("No policy assignments — no governance guardrails in place".to_string());
    }
    if regions.len() > 3 {
        warnings.push(format!(
            "Resources span {} regions — review for sprawl / latency / cost",
            regions.len()
        ));
    }
    if let Some(primary) = regions.first() {
        let outside: usize = regions.iter().skip(1).map(|c| c.count).sum();
        if outside > 0 && regions.len() > 1 {
            warnings.push(format!(
                "{outside} resource(s) outside the primary region '{}'",
                primary.name
            ));
        }
    }
    if has_pip {
        warnings.push("Public IPs present — confirm intended internet exposure".to_string());
    }

    Profile {
        subscription: subscription.to_string(),
        resource_count: total,
        resource_groups: rg_count,
        services,
        regions,
        naming,
        tagging,
        governance,
        signals,
        warnings,
    }
}

/// Serialize a profile into a compact, model-ready prompt (used by the Phase 2 GGUF advisor).
pub fn build_prompt(p: &Profile) -> String {
    let services = p
        .services
        .iter()
        .take(15)
        .map(|c| {
            format!(
                "{}×{}",
                c.count,
                c.name.rsplit('/').next().unwrap_or(&c.name)
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    let tags = p
        .tagging
        .iter()
        .map(|t| format!("{}={}%", t.tag, t.percent))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "You are an Azure architecture reviewer. Given this subscription profile, summarize what \
         architecture and patterns it follows, then list needs, worries, and opportunities, and \
         flag anything likely out of scope. Be concise.\n\n\
         Subscription: {}\nResources: {} in {} resource groups\nServices: {}\n\
         Regions: {}\nCAF naming conformance: {}%\nRequired-tag coverage: {}\n\
         Governance: {} policy assignments, {} non-compliant\nSignals: {}\nWarnings: {}\n",
        p.subscription,
        p.resource_count,
        p.resource_groups,
        services,
        p.regions
            .iter()
            .map(|c| c.name.clone())
            .collect::<Vec<_>>()
            .join(", "),
        p.naming.percent,
        tags,
        p.governance.assignments,
        p.governance.non_compliant,
        p.signals.join("; "),
        p.warnings.join("; "),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn maps_arm_types() {
        assert_eq!(
            arm_type_to_kind("Microsoft.Compute/virtualMachines"),
            Some("vm")
        );
        assert_eq!(
            arm_type_to_kind("Microsoft.Storage/storageAccounts"),
            Some("st")
        );
        assert_eq!(arm_type_to_kind("Microsoft.Foo/widgets"), None);
    }

    #[test]
    fn analyzes_naming_and_tags() {
        let resources = json!([
            { "name": "vm-app-prod-weu-001", "type": "Microsoft.Compute/virtualMachines",
              "location": "westeurope", "tags": { "environment": "prod", "owner": "x" } },
            { "name": "badname", "type": "Microsoft.Compute/virtualMachines",
              "location": "westeurope", "tags": {} },
        ]);
        let p = analyze(&resources, &json!([{}]), &json!([]), "sub1");
        assert_eq!(p.resource_count, 2);
        assert_eq!(p.naming.total, 2);
        assert_eq!(p.naming.conforming, 1);
        assert_eq!(p.naming.percent, 50);
        // environment covered on 1/2 = 50%; owner 1/2; others 0% -> all required tags warn (<80).
        let env = p.tagging.iter().find(|t| t.tag == "environment").unwrap();
        assert_eq!(env.percent, 50);
        // no governance -> warning present.
        assert!(p
            .warnings
            .iter()
            .any(|w| w.contains("No policy assignments")));
    }
}
