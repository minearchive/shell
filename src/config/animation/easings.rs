use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[allow(unused)]
#[derive(Deserialize, Serialize, Default)]
#[serde(default)]
pub(super) struct RawAnimationConfig {
    pub easings: HashMap<String, String>,
}
