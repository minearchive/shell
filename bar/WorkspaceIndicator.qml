import QtQuick
import QtQuick.Effects
import Quickshell.Hyprland
import "../assets"

Row {
    spacing: 5

    Repeater {
        model: 5

        Item {
            id: delegateItem
            required property int index

            width: 12
            height: 12

            RectangularShadow {
                anchors.fill: c
                radius: c.radius
                color: DeepSpacePalette.nebulaGlow
                blur: 5
                spread: 0
                offset: Qt.vector2d(0, 5)
            }

            Rectangle {
                id: c

                anchors.fill: parent
                radius: 10

                color: {
                    if (delegateItem.index === (Hyprland.focusedWorkspace.id - 1 % 5))
                        return DeepSpacePalette.nebula;
                    return "transparent";
                }

                border {
                    width: 3
                    color: {
                        if (delegateItem.index === (Hyprland.focusedWorkspace.id - 1 % 5))
                            return DeepSpacePalette.nebula;
                        return DeepSpacePalette.textDim;
                    }
                }

                Behavior on color {
                    ColorAnimation {
                        duration: DeepSpacePalette.durationSlow
                        easing.type: Easing.OutCirc
                    }
                }

                Behavior on border.color {
                    ColorAnimation {
                        duration: DeepSpacePalette.durationSlow
                        easing.type: Easing.OutCirc
                    }
                }

                MouseArea {
                    anchors.fill: parent
                    onClicked: {
                        Hyprland.dispatch(`workspace ${delegateItem.index + 1}`);
                    }
                }
            }
        }
    }
}
