pragma Singleton
import QtQuick 2.15

// ╔══════════════════════════════════════════════════════════╗
// ║  Deep Space Palette  —  singleton                        ║
// ║                                                          ║
// ║  Usage:                                                  ║
// ║    1. Register in main.cpp:                              ║
// ║         qmlRegisterSingletonType(QUrl::fromLocalFile(    ║
// ║           "DeepSpacePalette.qml"), "DS", 1, 0,           ║
// ║           "Palette");                                    ║
// ║    Or via qmldir:                                        ║
// ║         singleton DeepSpacePalette 1.0                   ║
// ║           DeepSpacePalette.qml                           ║
// ║                                                          ║
// ║    2. Import and use:                                    ║
// ║         import DS 1.0                                    ║
// ║         color: Palette.nebula                            ║
// ╚══════════════════════════════════════════════════════════╝

QtObject {

    // ── Backgrounds ─────────────────────────────────────────
    // Deep void base — bar / window background
    readonly property color bg: "#06060e"
    // Slightly lifted surface — cards, panels, dropdowns
    readonly property color surface: "#0b0b18"
    // Secondary surface — nested cards, hover states
    readonly property color surface2: "#0e0e20"
    // Deepest tint — input background, code blocks
    readonly property color well: "#030309"

    // ── Borders ─────────────────────────────────────────────
    // Default border (13% opacity blue-indigo)
    readonly property color border: Qt.rgba(80 / 255, 100 / 255, 180 / 255, 0.13)
    // Hovered / focused border
    readonly property color borderHover: Qt.rgba(80 / 255, 100 / 255, 180 / 255, 0.32)
    // Active / pressed border
    readonly property color borderActive: Qt.rgba(90 / 255, 74 / 255, 154 / 255, 0.50)
    readonly property color borderSolid: "#10122a"   // border を bg に合成済み
    readonly property color borderMid: Qt.rgba(80 / 255, 100 / 255, 180 / 255, 0.50)
    // ── Text ─────────────────────────────────────────────────
    // Primary readable text — blue-grey
    readonly property color text: "#7080a8"
    // Dimmed / inactive labels
    readonly property color textDim: "#2e3650"
    // Barely visible structural guides
    readonly property color textGhost: "#1a2038"
    // Bright emphasis — clock, values, active titles
    readonly property color star: "#c8c8e0"

    // ── Accent — Nebula (purple) ─────────────────────────────
    // Active workspace dot, primary CTA, focused ring
    readonly property color nebula: "#5a4a9a"
    readonly property color nebulaLight: "#a090e0"
    readonly property color nebulaDim: Qt.rgba(90 / 255, 74 / 255, 154 / 255, 0.15)
    readonly property color nebulaGlow: Qt.rgba(90 / 255, 74 / 255, 154 / 255, 0.40)

    // ── Accent — Ocean (blue) ────────────────────────────────
    // CPU, network, info states
    readonly property color ocean: "#2a5a8a"
    readonly property color oceanLight: "#7ab0d4"
    readonly property color oceanDim: Qt.rgba(42 / 255, 90 / 255, 138 / 255, 0.15)
    readonly property color oceanGlow: Qt.rgba(42 / 255, 90 / 255, 138 / 255, 0.35)

    // ── Accent — Mars (orange-red) ───────────────────────────
    // Temperature, warnings, destructive actions
    readonly property color mars: "#c87a5a"
    readonly property color marsLight: "#e0a080"
    readonly property color marsDim: Qt.rgba(200 / 255, 122 / 255, 90 / 255, 0.15)
    readonly property color marsGlow: Qt.rgba(200 / 255, 122 / 255, 90 / 255, 0.35)

    // ── Accent — Aurora (green) ──────────────────────────────
    // Memory, OK / success states, live indicators
    readonly property color aurora: "#4a9a7a"
    readonly property color auroraLight: "#6ac0a0"
    readonly property color auroraDim: Qt.rgba(74 / 255, 154 / 255, 122 / 255, 0.12)
    readonly property color auroraGlow: Qt.rgba(74 / 255, 154 / 255, 122 / 255, 0.32)

    // ── Semantic aliases ─────────────────────────────────────
    readonly property color colorSuccess: aurora
    readonly property color colorWarning: mars
    readonly property color colorInfo: oceanLight
    readonly property color colorPrimary: nebula
    readonly property color colorFocus: nebulaLight

    // ── Background nebula glows (radial gradient helpers) ────
    // Use these with ShaderEffect or Canvas for atmospheric depth
    readonly property color nebulaGlowLeft: Qt.rgba(90 / 255, 74 / 255, 154 / 255, 0.08)
    readonly property color nebulaGlowRight: Qt.rgba(42 / 255, 90 / 255, 138 / 255, 0.06)

    // ── Opacity levels ───────────────────────────────────────
    // Consistent opacity scale for overlays / backdrops
    readonly property real opacityBackdrop: 0.55
    readonly property real opacitySubtle: 0.08
    readonly property real opacityMid: 0.20
    readonly property real opacityStrong: 0.70

    // ── Radius ───────────────────────────────────────────────
    readonly property real radiusSm: 4
    readonly property real radiusMd: 6
    readonly property real radiusLg: 10
    readonly property real radiusXl: 14
    readonly property real radiusPill: 999

    // ── Typography ───────────────────────────────────────────
    readonly property string fontMono: "BlexMono Nerd Font"
    readonly property int fontWeightLight: 300
    readonly property int fontWeightNormal: 400
    readonly property int fontWeightMedium: 500

    readonly property real fontSizeXs: 8
    readonly property real fontSizeSm: 9
    readonly property real fontSizeMd: 11   // base / waybar
    readonly property real fontSizeLg: 13
    readonly property real fontSizeXl: 18
    readonly property real fontSizeHero: 40  // big clock

    // ── Motion ───────────────────────────────────────────────
    readonly property int durationFast: 150  // ms — hover transitions
    readonly property int durationNormal: 260  // ms — panel open/close
    readonly property int durationSlow: 400  // ms — staggered reveals

    // ── Helper: semantic color by state string ────────────────
    // usage: color: Palette.stateColor("ok")
    function stateColor(state) {
        switch (state) {
        case "ok":
            return aurora;
        case "warn":
            return mars;
        case "info":
            return oceanLight;
        case "active":
            return nebulaLight;
        case "dim":
            return textDim;
        default:
            return text;
        }
    }

    // ── Helper: border color by interaction state ─────────────
    function borderColor(hovered, pressed, focused) {
        if (pressed || focused)
            return borderActive;
        if (hovered)
            return borderHover;
        return border;
    }
}
