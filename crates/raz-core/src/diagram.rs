//! Topology diagrams from the resources raz lists. Mermaid renderer (`to_mermaid`) over a shared
//! topology model; edges from network resource details (vnet→subnet→nic→vm). The ASCII renderer is
//! feature-flagged off for now — see the note above `to_mermaid`.

use std::collections::HashMap;

use serde_json::Value;

use crate::advisor::arm_type_to_kind;

pub struct Node {
    /// Lowercased ARM id, used for edge matching.
    pub id: String,
    pub name: String,
    pub kind: String,
    pub rg: String,
}

pub struct Edge {
    pub from: String,
    pub to: String,
}

pub struct Topology {
    pub subscription: String,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    /// Resource group -> node indices (preserves first-seen order).
    pub groups: Vec<(String, Vec<usize>)>,
}

fn lower(s: &str) -> String {
    s.to_ascii_lowercase()
}

fn str_at<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

/// Build the topology from the resource list plus optional vnet/nic details (full ARM resources).
pub fn build(subscription: &str, resources: &Value, vnets: &[Value], nics: &[Value]) -> Topology {
    let mut nodes: Vec<Node> = Vec::new();
    let mut groups: Vec<(String, Vec<usize>)> = Vec::new();

    let push_group = |groups: &mut Vec<(String, Vec<usize>)>, rg: &str, idx: usize| match groups
        .iter_mut()
        .find(|(g, _)| g == rg)
    {
        Some((_, v)) => v.push(idx),
        None => groups.push((rg.to_string(), vec![idx])),
    };

    // Resource nodes.
    if let Some(arr) = resources.as_array() {
        for r in arr {
            let id = lower(str_at(r, "id"));
            if id.is_empty() {
                continue;
            }
            let typ = str_at(r, "type");
            let kind = arm_type_to_kind(typ)
                .map(str::to_string)
                .unwrap_or_else(|| typ.rsplit('/').next().unwrap_or(typ).to_string());
            let rg = {
                let g = str_at(r, "resourceGroup");
                if g.is_empty() {
                    crate::arm::resource_group_from_id(&id).unwrap_or_default()
                } else {
                    g.to_string()
                }
            };
            let idx = nodes.len();
            nodes.push(Node {
                id,
                name: str_at(r, "name").to_string(),
                kind,
                rg: rg.clone(),
            });
            push_group(&mut groups, &rg, idx);
        }
    }

    let mut edges: Vec<Edge> = Vec::new();

    // VNet -> subnet (subnets are sub-resources, added as nodes).
    for v in vnets {
        let vid = lower(str_at(v, "id"));
        if vid.is_empty() {
            continue;
        }
        let vrg = {
            let g = str_at(v, "resourceGroup");
            if g.is_empty() {
                crate::arm::resource_group_from_id(&vid).unwrap_or_default()
            } else {
                g.to_string()
            }
        };
        if let Some(subnets) = v.pointer("/properties/subnets").and_then(Value::as_array) {
            for sn in subnets {
                let sname = str_at(sn, "name");
                if sname.is_empty() {
                    continue;
                }
                let sid = lower(&format!("{vid}/subnets/{sname}"));
                let idx = nodes.len();
                nodes.push(Node {
                    id: sid.clone(),
                    name: sname.to_string(),
                    kind: "snet".to_string(),
                    rg: vrg.clone(),
                });
                push_group(&mut groups, &vrg, idx);
                edges.push(Edge {
                    from: vid.clone(),
                    to: sid,
                });
            }
        }
    }

    // NIC -> subnet membership and NIC -> VM attachment.
    for n in nics {
        let nid = lower(str_at(n, "id"));
        if nid.is_empty() {
            continue;
        }
        if let Some(subnet) = n
            .pointer("/properties/ipConfigurations/0/properties/subnet/id")
            .and_then(Value::as_str)
        {
            edges.push(Edge {
                from: lower(subnet),
                to: nid.clone(),
            });
        }
        if let Some(vm) = n
            .pointer("/properties/virtualMachine/id")
            .and_then(Value::as_str)
        {
            edges.push(Edge {
                from: nid.clone(),
                to: lower(vm),
            });
        }
    }

    Topology {
        subscription: subscription.to_string(),
        nodes,
        edges,
        groups,
    }
}

/// Map each node id to its index, for edge resolution.
fn id_index(t: &Topology) -> HashMap<&str, usize> {
    t.nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id.as_str(), i))
        .collect()
}

/// Render as a Mermaid `graph TD` inside a fenced code block.
pub fn to_mermaid(t: &Topology) -> String {
    let idx = id_index(t);
    let mut s = String::from("```mermaid\ngraph TD\n");
    for (rg, members) in &t.groups {
        s.push_str(&format!("  subgraph \"{rg}\"\n"));
        for &i in members {
            let n = &t.nodes[i];
            s.push_str(&format!("    n{i}[\"{}<br/>{}\"]\n", n.name, n.kind));
        }
        s.push_str("  end\n");
    }
    for e in &t.edges {
        if let (Some(&a), Some(&b)) = (idx.get(e.from.as_str()), idx.get(e.to.as_str())) {
            s.push_str(&format!("  n{a} --> n{b}\n"));
        }
    }
    s.push_str("```\n");
    s
}

// ---------------------------------------------------------------------------
// ASCII diagram renderer — FEATURE FLAGGED OFF for the moment.
//
// An indented Unicode-tree renderer (`to_ascii`: subscription → RG → vnet → subnet
// → nic → vm, using the same edges as `to_mermaid`) was built here for the planned
// TUI "Diagram" view. That consumer hasn't shipped, so rather than carry ~55 lines
// of dead code the renderer is parked. The topology model + edges it needs are
// already produced by `build()` above. Restore from git history (commit e94a946)
// or rebuild when the TUI Diagram tab lands.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fixture() -> Topology {
        let resources = json!([
            { "id": "/subscriptions/s/resourceGroups/rg1/providers/Microsoft.Network/virtualNetworks/hub",
              "name": "hub", "type": "Microsoft.Network/virtualNetworks", "resourceGroup": "rg1" },
            { "id": "/subscriptions/s/resourceGroups/rg1/providers/Microsoft.Network/networkInterfaces/vm1-nic",
              "name": "vm1-nic", "type": "Microsoft.Network/networkInterfaces", "resourceGroup": "rg1" },
            { "id": "/subscriptions/s/resourceGroups/rg1/providers/Microsoft.Compute/virtualMachines/vm1",
              "name": "vm1", "type": "Microsoft.Compute/virtualMachines", "resourceGroup": "rg1" },
        ]);
        let vnets = vec![json!({
            "id": "/subscriptions/s/resourceGroups/rg1/providers/Microsoft.Network/virtualNetworks/hub",
            "resourceGroup": "rg1",
            "properties": { "subnets": [ { "name": "default" } ] }
        })];
        let nics = vec![json!({
            "id": "/subscriptions/s/resourceGroups/rg1/providers/Microsoft.Network/networkInterfaces/vm1-nic",
            "properties": {
                "ipConfigurations": [ { "properties": { "subnet": {
                    "id": "/subscriptions/s/resourceGroups/rg1/providers/Microsoft.Network/virtualNetworks/hub/subnets/default" } } } ],
                "virtualMachine": { "id": "/subscriptions/s/resourceGroups/rg1/providers/Microsoft.Compute/virtualMachines/vm1" }
            }
        })];
        build("s", &resources, &vnets, &nics)
    }

    #[test]
    fn builds_network_edges() {
        let t = fixture();
        // 3 resources + 1 subnet node.
        assert_eq!(t.nodes.len(), 4);
        // vnet->subnet, subnet->nic, nic->vm.
        assert_eq!(t.edges.len(), 3);
    }

    #[test]
    fn renders_mermaid() {
        let t = fixture();
        let m = to_mermaid(&t);
        assert!(m.contains("```mermaid"));
        assert!(m.contains("graph TD"));
        assert!(m.contains("-->"));
    }
}
