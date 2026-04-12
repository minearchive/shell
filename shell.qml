import Quickshell
import QtQuick

import "assets"
import "bar"
import "window"

// import "widgets"

ShellRoot {
    Scope {
        Variants {
            id: root
            model: Quickshell.screens

            delegate: Component {
                // qmllint disable uncreatable-type
                PanelWindow {
                    id: shell

                    property var modelData
                    screen: modelData

                    property bool bottomMode: false

                    anchors {
                        top: !bottomMode
                        left: true
                        right: true
                        bottom: bottomMode
                    }

                    // qmllint disable unresolved-type
                    // qmllint disable unqualified
                    // qmllint disable missing-property
                    margins {
                        top: !bottomMode ? 10 : 0
                        left: 10
                        right: 10
                        bottom: bottomMode ? 10 : 0
                    }
                    // qmllint enable missing-property
                    // qmllint enable unqualified
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
            }
        }
    }

    AudioPopup {}
}
