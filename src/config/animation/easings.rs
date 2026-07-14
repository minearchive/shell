use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Default)]
#[serde(default)]
pub(super) struct RawAnimationConfig {
    pub easings: HashMap<String, String>,
}
