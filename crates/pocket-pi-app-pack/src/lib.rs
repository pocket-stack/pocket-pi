use anyhow::Result;
use pocket_pi_agentos::{AppCatalog, EmbeddedApp};

include!(concat!(env!("OUT_DIR"), "/apps.rs"));

pub fn catalog() -> Result<AppCatalog> {
    AppCatalog::new(embedded_apps())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn catalog_contains_exactly_the_build_selected_apps() {
        let catalog = catalog().unwrap();
        let mut actual = catalog
            .descriptors()
            .map(|descriptor| descriptor.id.as_str())
            .collect::<Vec<_>>();
        actual.sort_unstable();
        let mut expected = vec!["pi-agent"];
        let selected = option_env!("POCKET_PI_APPS").unwrap_or("robinhood,exa");
        if selected != "none" && !selected.is_empty() {
            expected.extend(selected.split(',').map(str::trim));
        }
        expected.sort_unstable();
        assert_eq!(actual, expected);
    }

    #[test]
    fn robinhood_catalog_matches_its_checked_in_snapshot() {
        let catalog = catalog().unwrap();
        let Some(descriptor) = catalog.descriptor("robinhood") else {
            return;
        };
        let snapshot: serde_json::Value =
            serde_json::from_str(include_str!("../../../apps/robinhood/tool-catalog.json"))
                .unwrap();
        let upstream = snapshot["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap().to_owned())
            .collect::<BTreeSet<_>>();
        let allowed = descriptor
            .provider_operations
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        assert_eq!(allowed, upstream);
        assert_eq!(upstream.len(), 54);
        assert_eq!(descriptor.tools.len(), 3);
    }
}
