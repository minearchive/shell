use std::collections::HashMap;
use std::fmt::{self, Display};
use std::fs;

use serde::{Deserialize, Serialize};

#[allow(unused)]
#[derive(Deserialize, Serialize)]
pub struct Configuration {
    pub name: String,
    pub dark: ColorTheme,
    pub light: ColorTheme,
}

#[allow(unused)]
#[derive(Deserialize, Serialize, Default)]
pub struct ColorTheme {
    pub primary: Option<String>,
    pub on_primary: Option<String>,
    pub primary_container: Option<String>,
    pub on_primary_container: Option<String>,
    pub inverse_primary: Option<String>,
    pub primary_fixed: Option<String>,
    pub primary_fixed_dim: Option<String>,
    pub on_primary_fixed: Option<String>,
    pub on_primary_fixed_variant: Option<String>,
    pub secondary: Option<String>,
    pub on_secondary: Option<String>,
    pub secondary_container: Option<String>,
    pub on_secondary_container: Option<String>,
    pub secondary_fixed: Option<String>,
    pub secondary_fixed_dim: Option<String>,
    pub on_secondary_fixed: Option<String>,
    pub on_secondary_fixed_variant: Option<String>,
    pub tertiary: Option<String>,
    pub on_tertiary: Option<String>,
    pub tertiary_container: Option<String>,
    pub on_tertiary_container: Option<String>,
    pub tertiary_fixed: Option<String>,
    pub tertiary_fixed_dim: Option<String>,
    pub on_tertiary_fixed: Option<String>,
    pub on_tertiary_fixed_variant: Option<String>,
    pub source_color: Option<String>,
    pub error: Option<String>,
    pub on_error: Option<String>,
    pub error_container: Option<String>,
    pub on_error_container: Option<String>,
    pub surface_dim: Option<String>,
    pub surface: Option<String>,
    pub surface_bright: Option<String>,
    pub surface_container_lowest: Option<String>,
    pub surface_container_low: Option<String>,
    pub surface_container: Option<String>,
    pub surface_container_high: Option<String>,
    pub surface_container_highest: Option<String>,
    pub on_surface: Option<String>,
    pub on_surface_variant: Option<String>,
    pub outline: Option<String>,
    pub outline_variant: Option<String>,
    pub inverse_surface: Option<String>,
    pub inverse_on_surface: Option<String>,
    pub surface_variant: Option<String>,
    pub background: Option<String>,
    pub on_background: Option<String>,
    pub shadow: Option<String>,
    pub scrim: Option<String>,
}

impl Default for Configuration {
    fn default() -> Self {
        Self {
            name: Default::default(),
            light: Default::default(),
            dark: Default::default(),
        }
    }
}

impl Configuration {
    pub fn load(path: &str) -> Self {
        Self::load_template(path)
    }

    pub fn load_template(path: &str) -> Self {
        let src = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                println!("Configuration file load failed, using defaults: {e}");
                return Self::default();
            }
        };

        match toml::from_str::<Configuration>(&src) {
            Ok(cfg) => cfg,
            Err(e) => {
                println!("Configuration parse failed, check file syntax:\n{e}");
                Self::default()
            }
        }
    }
}
