import Quickshell
import QtQuick
import QtQuick.Layouts

import "assets"
import "bar"

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
                    }

                    implicitHeight: 40

                    PopupWindow {
                        anchor.window: shell
                        anchor.rect.x: shell.width / 2 - width / 2
                        anchor.rect.y: shell.height + 20
                        implicitWidth: 700
                        implicitHeight: 420
                        // visible: true

                        color: "transparent"

                        onVisibleChanged: {
                            inner.visible = visible;
                        }

                        Rectangle {
                            id: inner

                            color: DeepSpacePalette.bg
                            anchors.fill: parent

                            border {
                                color: DeepSpacePalette.borderMid
                            }

                            RowLayout {
                                anchors.fill: parent
                                anchors.margins: 16
                                spacing: 16

                                SysInfoPanel {
                                    Layout.preferredWidth: (parent.width - 1) / 3
                                    Layout.fillHeight: true
                                }

                                Rectangle {
                                    Layout.preferredWidth: 1
                                    Layout.fillHeight: true
                                    color: DeepSpacePalette.borderMid
                                    opacity: 0.5
                                }

                                CalendarWidget {
                                    Layout.fillWidth: true
                                    Layout.fillHeight: true
                                }
                            }
                        }
                    }
                }
                // qmllint enable uncreatable-type
            }
        }
    }
}
