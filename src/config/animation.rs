use std::collections::HashMap;
use std::path::Path;

use log::warn;
use serde::Deserialize;

use ui_core::animation::parser::{css_to_easing, Easing};

use super::watchable::{read_toml, WatchableConfig};

/// Prefix of a value that reuses another easing by name: `"conf::<name>"`.
const REF_PREFIX: &str = "conf::";

/// Follows `conf::` references until it reaches CSS.
fn resolve<'a>(raw: &'a HashMap<String, String>, mut value: &'a str) -> Result<&'a str, String> {
    let mut seen = Vec::new();
    while let Some(target) = value.trim().strip_prefix(REF_PREFIX) {
        if seen.contains(&target) {
            return Err(format!("reference cycle through {target:?}"));
        }
        seen.push(target);
        value = raw
            .get(target)
            .ok_or_else(|| format!("undefined easing {target:?}"))?;
    }
    Ok(value)
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RawAnimationConfig {
    easings: HashMap<String, String>,
}

#[derive(Clone, Default)]
pub struct AnimationConfig {
    raw: HashMap<String, String>,
    easings: HashMap<String, Easing>,
}

impl WatchableConfig for AnimationConfig {
    fn try_load(path: &Path) -> Option<Self> {
        let raw_cfg: RawAnimationConfig = read_toml(path)?;

        let mut easings = HashMap::new();
        for (name, value) in &raw_cfg.easings {
            let parsed = resolve(&raw_cfg.easings, value)
                .and_then(|css| css_to_easing(css).map_err(|e| e.to_string()));
            match parsed {
                Ok(easing) => {
                    easings.insert(name.clone(), easing);
                }
                Err(e) => warn!("easing {name:?}: {e}"),
            }
        }

        Some(Self {
            raw: raw_cfg.easings,
            easings,
        })
    }

    fn is_changed(&self, new: &Self) -> bool {
        self.raw != new.raw
    }
}

impl AnimationConfig {
    pub fn easings(&self) -> &HashMap<String, Easing> {
        &self.easings
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_references() {
        let raw: HashMap<String, String> = [
            ("base", "ease-in"),
            ("alias", "conf::base"),
            ("chain", "conf::alias"),
            ("missing", "conf::nope"),
            ("loop_a", "conf::loop_b"),
            ("loop_b", "conf::loop_a"),
            ("self", "conf::self"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();

        assert_eq!(resolve(&raw, "linear"), Ok("linear"));
        assert_eq!(resolve(&raw, &raw["chain"]), Ok("ease-in"));
        assert!(resolve(&raw, &raw["missing"]).is_err());
        assert!(resolve(&raw, &raw["loop_a"]).is_err());
        assert!(resolve(&raw, &raw["self"]).is_err());
    }
}
