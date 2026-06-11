import Quickshell
import Quickshell.Wayland
import QtQuick
import QtQuick.Layouts

import "assets"
import "bar"
import "window"

// import "widgets"

ShellRoot {
    id: shellRoot
    property bool showWatermark: false

    Variants {
        model: Quickshell.screens

        delegate: Component {
            Scope {
                id: delegateScope
                required property var modelData

                // qmllint disable uncreatable-type
                PanelWindow {
                    id: shell

                    property bool bottomMode: false
                    screen: delegateScope.modelData

                    anchors {
                        top: !bottomMode
                        left: true
                        right: true
                        bottom: bottomMode
                    }

                    // qmllint disable unresolved-type
                    // qmllint disable missing-property
                    margins {
                        top: !bottomMode ? 10 : 0
                        left: 10
                        right: 10
                        bottom: bottomMode ? 10 : 0
                    }
                    // qmllint enable missing-property
                    // qmllint enable unresolved-type

                    color: DeepSpacePalette.bg

                    Rectangle {
                        anchors.fill: parent
                        color: DeepSpacePalette.bg

                        border {
                            color: DeepSpacePalette.borderMid
                        }

                        WorkspaceIndicator {
                            anchors {
                                left: parent.left
                                verticalCenter: parent.verticalCenter
                                leftMargin: 10
                            }
                        }

                        Clock {
                            anchors {
                                verticalCenter: parent.verticalCenter
                                horizontalCenter: parent.horizontalCenter
                            }
                        }

                        SystemStatus {
                            anchors {
                                verticalCenter: parent.verticalCenter
                                right: parent.right
                                rightMargin: 10
                            }
                        }

                        // Notification {}
                    }

                    implicitHeight: 40
                }
                // qmllint enable uncreatable-type

                LazyLoader {
                    active: shellRoot.showWatermark

                    // qmllint disable uncreatable-type
                    PanelWindow {
                        screen: delegateScope.modelData

                        anchors {
                            right: true
                            bottom: true
                        }

                        // qmllint disable unresolved-type
                        // qmllint disable missing-property
                        margins {
                            right: 50
                            bottom: 50
                        }
                        // qmllint enable missing-property
                        // qmllint enable unresolved-type

                        implicitWidth: content.width
                        implicitHeight: content.height

                        color: "transparent"

                        mask: Region {}

                        WlrLayershell.layer: WlrLayer.Overlay

                        ColumnLayout {
                            id: content

                            Text {
                                text: "Activate Linux"
                                color: "#50ffffff"
                                font.pointSize: 22
                            }

                            Text {
                                text: "Go to Settings to activate Linux"
                                color: "#50ffffff"
                                font.pointSize: 14
                            }
                        }
                    }
                    // qmllint enable uncreatable-type
                }
            } // Scope
        }
    }

    AudioPopup {}
}
