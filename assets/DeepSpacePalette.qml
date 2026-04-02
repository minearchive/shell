pragma Singleton
import QtQuick 2.15

QtObject {

    readonly property color bg: "#06060e"
    readonly property color surface: "#0b0b18"
    readonly property color surface2: "#0e0e20"
    readonly property color well: "#030309"

    readonly property color border: Qt.rgba(80 / 255, 100 / 255, 180 / 255, 0.13)
    readonly property color borderHover: Qt.rgba(80 / 255, 100 / 255, 180 / 255, 0.32)
    readonly property color borderActive: Qt.rgba(90 / 255, 74 / 255, 154 / 255, 0.50)
    readonly property color borderSolid: "#10122a"
    readonly property color borderMid: Qt.rgba(80 / 255, 100 / 255, 180 / 255, 0.50)

    readonly property color text: "#7080a8"
    readonly property color textDim: "#2e3650"
    readonly property color textGhost: "#1a2038"
    readonly property color star: "#c8c8e0"

    readonly property color nebula: "#5a4a9a"
    readonly property color nebulaLight: "#a090e0"
    readonly property color nebulaDim: Qt.rgba(90 / 255, 74 / 255, 154 / 255, 0.15)
    readonly property color nebulaGlow: Qt.rgba(90 / 255, 74 / 255, 154 / 255, 0.40)

    readonly property color ocean: "#2a5a8a"
    readonly property color oceanLight: "#7ab0d4"
    readonly property color oceanDim: Qt.rgba(42 / 255, 90 / 255, 138 / 255, 0.15)
    readonly property color oceanGlow: Qt.rgba(42 / 255, 90 / 255, 138 / 255, 0.35)

    readonly property color mars: "#c87a5a"
    readonly property color marsLight: "#e0a080"
    readonly property color marsDim: Qt.rgba(200 / 255, 122 / 255, 90 / 255, 0.15)
    readonly property color marsGlow: Qt.rgba(200 / 255, 122 / 255, 90 / 255, 0.35)

    readonly property color aurora: "#4a9a7a"
    readonly property color auroraLight: "#6ac0a0"
    readonly property color auroraDim: Qt.rgba(74 / 255, 154 / 255, 122 / 255, 0.12)
    readonly property color auroraGlow: Qt.rgba(74 / 255, 154 / 255, 122 / 255, 0.32)

    readonly property color colorSuccess: aurora
    readonly property color colorWarning: mars
    readonly property color colorInfo: oceanLight
    readonly property color colorPrimary: nebula
    readonly property color colorFocus: nebulaLight

    readonly property color nebulaGlowLeft: Qt.rgba(90 / 255, 74 / 255, 154 / 255, 0.08)
    readonly property color nebulaGlowRight: Qt.rgba(42 / 255, 90 / 255, 138 / 255, 0.06)

    readonly property real opacityBackdrop: 0.55
    readonly property real opacitySubtle: 0.08
    readonly property real opacityMid: 0.20
    readonly property real opacityStrong: 0.70

    readonly property real radiusSm: 4
    readonly property real radiusMd: 6
    readonly property real radiusLg: 10
    readonly property real radiusXl: 14
    readonly property real radiusPill: 999

    readonly property string fontMono: "BlexMono Nerd Font"
    readonly property int fontWeightLight: 300
    readonly property int fontWeightNormal: 400
    readonly property int fontWeightMedium: 500

    readonly property real fontSizeXs: 8
    readonly property real fontSizeSm: 9
    readonly property real fontSizeMd: 11
    readonly property real fontSizeLg: 13
    readonly property real fontSizeXl: 18
    readonly property real fontSizeHero: 40

    readonly property int durationFast: 150
    readonly property int durationNormal: 260
    readonly property int durationSlow: 400

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

    function borderColor(hovered, pressed, focused) {
        if (pressed || focused)
            return borderActive;
        if (hovered)
            return borderHover;
        return border;
    }
}
