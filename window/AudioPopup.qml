import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Services.Pipewire

import "../assets"

Scope {
    id: root

    PwObjectTracker {
        objects: [Pipewire.defaultAudioSink]
    }

    Connections {
        target: Pipewire.defaultAudioSink?.audio

        function onVolumeChanged() {
            root.shouldShow = true;
            hide.restart();
        }
    }

    property bool shouldShow: false

    Timer {
        id: hide
        interval: 3000
        onTriggered: {
            root.shouldShow = false;
        }
    }

    // qmllint disable uncreatable-type
    PanelWindow {
        anchors.top: true
        // qmllint disable unresolved-type
        // qmllint disable unqualified
        // qmllint disable missing-property
        margins.top: 20
        exclusiveZone: 0

        implicitWidth: 200
        implicitHeight: 40
        color: "transparent"

        mask: Region {}

        Rectangle {
            anchors.fill: parent
            color: DeepSpacePalette.bg
            border {
                color: DeepSpacePalette.borderMid
            }

            opacity: root.shouldShow ? 1.0 : 0.0
            scale: root.shouldShow ? 1.0 : 0.85

            Behavior on opacity {
                NumberAnimation {
                    duration: 180
                    easing.type: Easing.OutCubic
                }
            }

            Behavior on scale {
                NumberAnimation {
                    duration: 180
                    easing.type: Easing.OutCubic
                }
            }

            RowLayout {
                anchors {
                    fill: parent
                    leftMargin: 15
                    rightMargin: 15
                }

                Rectangle {
                    // Stretches to fill all left-over space
                    Layout.fillWidth: true

                    implicitHeight: 12
                    radius: 20
                    color: "#50ffffff"

                    Rectangle {
                        anchors {
                            left: parent.left
                            top: parent.top
                            bottom: parent.bottom
                        }

                        implicitWidth: parent.width * (Pipewire.defaultAudioSink?.audio.volume ?? 0)
                        radius: parent.radius

                        Behavior on implicitWidth {
                            NumberAnimation {
                                duration: 180
                                easing.type: Easing.OutCubic
                            }
                        }
                    }
                }
            }
        }
    }
}
