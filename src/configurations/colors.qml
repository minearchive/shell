pragma Singleton
import QtQuick

QtObject {
    // Primary Colors
    readonly property color primary: "#e6f1ff"
    readonly property color primary_fixed: "#cce5ff"
    readonly property color primary_fixed_dim: "#98ccf9"
    readonly property color on_primary: "#000000"
    readonly property color on_primary_fixed: "#000000"
    readonly property color on_primary_fixed_variant: "#001321"
    readonly property color primary_container: "#95c8f5"
    readonly property color on_primary_container: "#000000"

    // Secondary Colors
    readonly property color secondary: "#e6f1ff"
    readonly property color secondary_fixed: "#d4e4f6"
    readonly property color secondary_fixed_dim: "#b8c8da"
    readonly property color on_secondary: "#000000"
    readonly property color on_secondary_fixed: "#000000"
    readonly property color on_secondary_fixed_variant: "#03121f"
    readonly property color secondary_container: "#b4c4d6"
    readonly property color on_secondary_container: "#000000"

    // Tertiary Colors
    readonly property color tertiary: "#f7ecff"
    readonly property color tertiary_fixed: "#eddcff"
    readonly property color tertiary_fixed_dim: "#d1bfe7"
    readonly property color on_tertiary: "#000000"
    readonly property color on_tertiary_fixed: "#000000"
    readonly property color on_tertiary_fixed_variant: "#170a29"
    readonly property color tertiary_container: "#cdbbe3"
    readonly property color on_tertiary_container: "#000000"

    // Error Colors
    readonly property color error: "#ffece9"
    readonly property color on_error: "#000000"
    readonly property color error_container: "#ffaea4"
    readonly property color on_error_container: "#000000"

    // Surface & Background Colors
    readonly property color surface: "#101418"
    readonly property color on_surface: "#ffffff"
    readonly property color on_surface_variant: "#ffffff"
    readonly property color surface_dim: "#101418"
    readonly property color surface_bright: "#4d5055"
    readonly property color surface_container_lowest: "#000000"
    readonly property color surface_container_low: "#1c2024"
    readonly property color surface_container: "#2d3135"
    readonly property color surface_container_high: "#383c40"
    readonly property color surface_container_highest: "#43474b"

    // Inverse Colors
    readonly property color inverse_surface: "#e0e2e8"
    readonly property color inverse_on_surface: "#000000"
    readonly property color inverse_primary: "#094c73"

    // Other Colors
    readonly property color outline: "#ebf0f8"
    readonly property color outline_variant: "#bec3ca"
    readonly property color shadow: "#000000"
    readonly property color scrim: "#000000"

    // Source Color
    readonly property color source_color: "#2c8dcc"

    // Background Colors
    readonly property color background: "#101418"
    readonly property color on_background: "#e0e2e8"
    readonly property color surface_variant: "#42474e"

    readonly property string mode : "dark"
}
