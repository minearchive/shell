use serde::{Deserialize, Serialize};

use super::color::Color;

#[allow(unused)]
#[derive(Deserialize, Serialize, Default)]
#[serde(default)]
pub struct ColorTheme {
    pub primary: Color,
    pub on_primary: Color,
    pub primary_container: Color,
    pub on_primary_container: Color,
    pub inverse_primary: Color,
    pub primary_fixed: Color,
    pub primary_fixed_dim: Color,
    pub on_primary_fixed: Color,
    pub on_primary_fixed_variant: Color,
    pub secondary: Color,
    pub on_secondary: Color,
    pub secondary_container: Color,
    pub on_secondary_container: Color,
    pub secondary_fixed: Color,
    pub secondary_fixed_dim: Color,
    pub on_secondary_fixed: Color,
    pub on_secondary_fixed_variant: Color,
    pub tertiary: Color,
    pub on_tertiary: Color,
    pub tertiary_container: Color,
    pub on_tertiary_container: Color,
    pub tertiary_fixed: Color,
    pub tertiary_fixed_dim: Color,
    pub on_tertiary_fixed: Color,
    pub on_tertiary_fixed_variant: Color,
    pub source_color: Color,
    pub error: Color,
    pub on_error: Color,
    pub error_container: Color,
    pub on_error_container: Color,
    pub surface_dim: Color,
    pub surface: Color,
    pub surface_bright: Color,
    pub surface_container_lowest: Color,
    pub surface_container_low: Color,
    pub surface_container: Color,
    pub surface_container_high: Color,
    pub surface_container_highest: Color,
    pub on_surface: Color,
    pub on_surface_variant: Color,
    pub outline: Color,
    pub outline_variant: Color,
    pub inverse_surface: Color,
    pub inverse_on_surface: Color,
    pub surface_variant: Color,
    pub background: Color,
    pub on_background: Color,
    pub shadow: Color,
    pub scrim: Color,
}

impl PartialEq for ColorTheme {
    fn eq(&self, other: &Self) -> bool {
        self.primary == other.primary
            && self.on_primary == other.on_primary
            && self.primary_container == other.primary_container
            && self.on_primary_container == other.on_primary_container
            && self.inverse_primary == other.inverse_primary
            && self.primary_fixed == other.primary_fixed
            && self.primary_fixed_dim == other.primary_fixed_dim
            && self.on_primary_fixed == other.on_primary_fixed
            && self.on_primary_fixed_variant == other.on_primary_fixed_variant
            && self.secondary == other.secondary
            && self.on_secondary == other.on_secondary
            && self.secondary_container == other.secondary_container
            && self.on_secondary_container == other.on_secondary_container
            && self.secondary_fixed == other.secondary_fixed
            && self.secondary_fixed_dim == other.secondary_fixed_dim
            && self.on_secondary_fixed == other.on_secondary_fixed
            && self.on_secondary_fixed_variant == other.on_secondary_fixed_variant
            && self.tertiary == other.tertiary
            && self.on_tertiary == other.on_tertiary
            && self.tertiary_container == other.tertiary_container
            && self.on_tertiary_container == other.on_tertiary_container
            && self.tertiary_fixed == other.tertiary_fixed
            && self.tertiary_fixed_dim == other.tertiary_fixed_dim
            && self.on_tertiary_fixed == other.on_tertiary_fixed
            && self.on_tertiary_fixed_variant == other.on_tertiary_fixed_variant
            && self.source_color == other.source_color
            && self.error == other.error
            && self.on_error == other.on_error
            && self.error_container == other.error_container
            && self.on_error_container == other.on_error_container
            && self.surface_dim == other.surface_dim
            && self.surface == other.surface
            && self.surface_bright == other.surface_bright
            && self.surface_container_lowest == other.surface_container_lowest
            && self.surface_container_low == other.surface_container_low
            && self.surface_container == other.surface_container
            && self.surface_container_high == other.surface_container_high
            && self.surface_container_highest == other.surface_container_highest
            && self.on_surface == other.on_surface
            && self.on_surface_variant == other.on_surface_variant
            && self.outline == other.outline
            && self.outline_variant == other.outline_variant
            && self.inverse_surface == other.inverse_surface
            && self.inverse_on_surface == other.inverse_on_surface
            && self.surface_variant == other.surface_variant
            && self.background == other.background
            && self.on_background == other.on_background
            && self.shadow == other.shadow
            && self.scrim == other.scrim
    }
}
