#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ResourceNavCategory {
    pub(crate) name: &'static str,
    pub(crate) icon: &'static str,
    pub(crate) items: &'static [ResourceNavItem],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ResourceNavItem {
    pub(crate) name: &'static str,
}

pub(crate) const RESOURCE_CATEGORIES: &[ResourceNavCategory] = &[
    ResourceNavCategory {
        name: "General",
        icon: egui_phosphor::regular::TREE_STRUCTURE,
        items: &[
            ResourceNavItem { name: "Nodes" },
            ResourceNavItem { name: "Namespaces" },
            ResourceNavItem { name: "Events" },
        ],
    },
    ResourceNavCategory {
        name: "Workloads",
        icon: egui_phosphor::regular::CUBE,
        items: &[
            ResourceNavItem { name: "Overview" },
            ResourceNavItem { name: "Pods" },
            ResourceNavItem {
                name: "Deployments",
            },
            ResourceNavItem {
                name: "Daemon Sets",
            },
            ResourceNavItem {
                name: "Stateful Sets",
            },
            ResourceNavItem {
                name: "Replica Sets",
            },
            ResourceNavItem {
                name: "Replication Controllers",
            },
            ResourceNavItem { name: "Jobs" },
            ResourceNavItem { name: "Cron Jobs" },
        ],
    },
    ResourceNavCategory {
        name: "Config",
        icon: egui_phosphor::regular::GEAR,
        items: &[
            ResourceNavItem {
                name: "Config Maps",
            },
            ResourceNavItem { name: "Secrets" },
            ResourceNavItem {
                name: "Resource Quotas",
            },
            ResourceNavItem {
                name: "Limit Ranges",
            },
            ResourceNavItem {
                name: "Horizontal Pod Autoscalers",
            },
            ResourceNavItem {
                name: "Pod Disruption Budgets",
            },
            ResourceNavItem {
                name: "Priority Classes",
            },
            ResourceNavItem {
                name: "Runtime Classes",
            },
            ResourceNavItem { name: "Leases" },
            ResourceNavItem {
                name: "Mutating Webhook Configurations",
            },
            ResourceNavItem {
                name: "Validating Webhook Configurations",
            },
        ],
    },
    ResourceNavCategory {
        name: "Network",
        icon: egui_phosphor::regular::ARROWS_DOWN_UP,
        items: &[
            ResourceNavItem { name: "Services" },
            ResourceNavItem {
                name: "Endpoint Slices",
            },
            ResourceNavItem { name: "Endpoints" },
            ResourceNavItem { name: "Ingresses" },
            ResourceNavItem {
                name: "Ingress Classes",
            },
            ResourceNavItem {
                name: "Network Policies",
            },
            ResourceNavItem {
                name: "Port Forwarding",
            },
        ],
    },
    ResourceNavCategory {
        name: "Storage",
        icon: egui_phosphor::regular::DATABASE,
        items: &[
            ResourceNavItem {
                name: "Persistent Volume Claims",
            },
            ResourceNavItem {
                name: "Persistent Volumes",
            },
            ResourceNavItem {
                name: "Storage Classes",
            },
        ],
    },
    ResourceNavCategory {
        name: "Access Control",
        icon: egui_phosphor::regular::SHIELD,
        items: &[
            ResourceNavItem {
                name: "Service Accounts",
            },
            ResourceNavItem {
                name: "Cluster Roles",
            },
            ResourceNavItem { name: "Roles" },
            ResourceNavItem {
                name: "Cluster Role Bindings",
            },
            ResourceNavItem {
                name: "Role Bindings",
            },
        ],
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_categories_are_in_expected_order() {
        let names: Vec<_> = RESOURCE_CATEGORIES
            .iter()
            .map(|category| category.name)
            .collect();

        assert_eq!(
            names,
            vec![
                "General",
                "Workloads",
                "Config",
                "Network",
                "Storage",
                "Access Control"
            ]
        );
    }

    #[test]
    fn resource_catalog_contains_representative_items() {
        assert_category_contains("General", "Nodes");
        assert_category_contains("General", "Namespaces");
        assert_category_contains("General", "Events");
        assert_category_contains("Workloads", "Pods");
        assert_category_contains("Config", "Config Maps");
        assert_category_contains("Network", "Endpoint Slices");
        assert_category_contains("Storage", "Persistent Volumes");
        assert_category_contains("Access Control", "Cluster Role Bindings");
    }

    fn assert_category_contains(category_name: &str, item_name: &str) {
        let category = RESOURCE_CATEGORIES
            .iter()
            .find(|category| category.name == category_name)
            .unwrap();

        assert!(category.items.iter().any(|item| item.name == item_name));
    }
}
