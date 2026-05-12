// qmllint disable import
import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Services.Mpris
import "../assets"
import "../singleton"

Item {
    id: root
    anchors.centerIn: parent

    property string nowPlaying: ""
    property bool showPlayer: false
    property bool showInfo: false
    property var player: null
    property string imageUrl: ""

    function updateNowPlaying() {
        const players = Mpris.players.values.filter(p => p.isPlaying);
        console.log("Updating");

        if (players.length > 0) {
            hideTimer.stop();
            player = players[0];
            nowPlaying = players.map(p => p.trackTitle).join(", ");
            imageUrl = player.trackArtUrl || "";
            showPlayer = true;
        } else {
            hideTimer.restart();
        }
    }

    Timer {
        id: hideTimer
        interval: 10000
        repeat: false
        onTriggered: root.showPlayer = false
    }

    Connections {
        target: Mpris.players
        function onValuesChanged() {
            root.updateNowPlaying();
        }
    }

    Repeater {
        model: Mpris.players.values
        delegate: Item {
            required property var modelData
            Connections {
                target: modelData
                function onIsPlayingChanged() {
                    root.updateNowPlaying();
                }
                function onPostTrackChanged() {
                    root.updateNowPlaying();
                }
                function onTrackArtUrlChanged() {
                    root.updateNowPlaying();
                }
            }
        }
    }

    width: time.implicitWidth + playerContainer.width
    height: time.implicitHeight

    Row {
        spacing: 0
        anchors.centerIn: parent

        Text {
            id: time
            text: Time.time
            font.pixelSize: 18
            font.family: DeepSpacePalette.fontMono
            color: DeepSpacePalette.star
        }

        Item {
            id: playerContainer
            height: playerText.implicitHeight
            width: root.showPlayer ? playerText.implicitWidth + 20 : 0
            clip: true

            Behavior on width {
                NumberAnimation {
                    duration: 750
                    easing.type: Easing.OutBack
                    easing.overshoot: 0.8
                }
            }

            Text {
                id: playerText
                anchors.left: parent.left
                anchors.leftMargin: 20
                anchors.verticalCenter: parent.verticalCenter
                text: root.nowPlaying
                font.pixelSize: 18
                font.family: DeepSpacePalette.fontMono
                color: DeepSpacePalette.star
                opacity: playerContainer.width / (implicitWidth + 20 || 1)
            }

            MouseArea {
                anchors.fill: parent
                onClicked: root.showInfo = !root.showInfo
            }
        }

        LazyLoader {
            active: root.showInfo

            PanelWindow {
                exclusiveZone: 0
                anchors.top: true

                implicitWidth: 380
                implicitHeight: visible ? 148 : 0
                color: "transparent"

                visible: root.showPlayer

                property var player: root.player

                function formatTime(us) {
                    var s = Math.floor(us / 1_000_000);
                    var m = Math.floor(s / 60);
                    s = s % 60;
                    return m + ":" + (s < 10 ? "0" : "") + s;
                }

                Behavior on implicitHeight {
                    NumberAnimation {
                        duration: 220
                        easing.type: Easing.OutCubic
                    }
                }

                Rectangle {
                    anchors.fill: parent
                    color: DeepSpacePalette.bg
                    border.color: DeepSpacePalette.borderMid
                    border.width: 1

                    // ── メインレイアウト ─────────────────────────────────
                    RowLayout {
                        anchors {
                            fill: parent
                            margins: 14
                        }
                        spacing: 14

                        // ─ アルバムアート ─────────────────────────────────
                        Item {
                            implicitWidth: 112
                            implicitHeight: 112
                            Layout.alignment: Qt.AlignVCenter

                            Rectangle {
                                anchors.fill: parent
                                radius: DeepSpacePalette.radiusMd
                                color: DeepSpacePalette.surface2

                                Text {
                                    anchors.centerIn: parent
                                    text: "♫"
                                    font.pixelSize: 32
                                    font.family: DeepSpacePalette.fontMono
                                    color: DeepSpacePalette.nebula
                                }
                            }

                            Rectangle {
                                anchors.fill: parent
                                radius: DeepSpacePalette.radiusMd
                                clip: true
                                color: "transparent"

                                Image {
                                    id: artImage
                                    anchors.fill: parent
                                    source: root.imageUrl
                                    fillMode: Image.PreserveAspectCrop
                                    asynchronous: true
                                    opacity: status === Image.Ready ? 1.0 : 0.0

                                    Behavior on opacity {
                                        NumberAnimation {
                                            duration: 200
                                            easing.type: Easing.OutCubic
                                        }
                                    }
                                }
                            }
                        }

                        // ─ テキスト + プログレス + コントロール ───────────
                        ColumnLayout {
                            Layout.fillWidth: true
                            Layout.fillHeight: true
                            spacing: 4

                            Text {
                                id: trackText_
                                Layout.fillWidth: true
                                text: player?.trackTitle ?? "—"
                                font.pixelSize: DeepSpacePalette.fontSizeLg
                                font.weight: Font.DemiBold
                                font.family: DeepSpacePalette.fontMono
                                color: DeepSpacePalette.star
                                elide: Text.ElideRight
                                maximumLineCount: 1

                                Behavior on text {
                                    SequentialAnimation {
                                        NumberAnimation {
                                            target: trackText_
                                            property: "opacity"
                                            to: 0
                                            duration: 80
                                        }
                                        PropertyAction {}
                                        NumberAnimation {
                                            target: trackText_
                                            property: "opacity"
                                            to: 1
                                            duration: 120
                                        }
                                    }
                                }
                            }

                            Text {
                                Layout.fillWidth: true
                                text: [player?.trackArtist, player?.trackAlbum].filter(Boolean).join(" · ")
                                font.pixelSize: DeepSpacePalette.fontSizeMd
                                font.family: DeepSpacePalette.fontMono
                                color: DeepSpacePalette.text
                                elide: Text.ElideRight
                                maximumLineCount: 1
                            }

                            // ─ プログレスバー ─
                            Item {
                                Layout.fillWidth: true
                                implicitHeight: 16

                                Rectangle {
                                    id: track
                                    anchors.verticalCenter: parent.verticalCenter
                                    width: parent.width
                                    height: 3
                                    radius: 2
                                    color: DeepSpacePalette.textGhost

                                    Rectangle {
                                        width: player ? (player.length > 0 ? Math.min(player.position / player.length, 1.0) * track.width : 0) : 0
                                        height: parent.height
                                        radius: 2
                                        color: DeepSpacePalette.nebula

                                        Behavior on width {
                                            SmoothedAnimation {
                                                velocity: 40
                                                duration: 800
                                            }
                                        }

                                        Rectangle {
                                            anchors {
                                                right: parent.right
                                                verticalCenter: parent.verticalCenter
                                            }
                                            width: 10
                                            height: 10
                                            radius: 5
                                            color: DeepSpacePalette.nebulaLight
                                            border.color: Qt.rgba(DeepSpacePalette.star.r, DeepSpacePalette.star.g, DeepSpacePalette.star.b, 0.6)
                                            border.width: 1.5
                                        }
                                    }
                                }

                                MouseArea {
                                    anchors.fill: parent
                                    onClicked: mouse => {
                                        if (player && player.length > 0) {
                                            var ratio = mouse.x / width;
                                            player.position = ratio * player.length;
                                        }
                                    }
                                    cursorShape: Qt.PointingHandCursor
                                }
                            }

                            // 時間表示
                            RowLayout {
                                Layout.fillWidth: true

                                Text {
                                    text: player ? formatTime(player.position) : "0:00"
                                    font.pixelSize: DeepSpacePalette.fontSizeSm
                                    font.family: DeepSpacePalette.fontMono
                                    color: DeepSpacePalette.textDim
                                }
                                Item {
                                    Layout.fillWidth: true
                                }
                                Text {
                                    text: player ? formatTime(player.length) : "0:00"
                                    font.pixelSize: DeepSpacePalette.fontSizeSm
                                    font.family: DeepSpacePalette.fontMono
                                    color: DeepSpacePalette.textDim
                                }
                            }

                            // ─ コントロールボタン ─
                            RowLayout {
                                Layout.fillWidth: true
                                spacing: 0

                                MediaButton {
                                    iconText: ""
                                    active: player?.shuffle ?? false
                                    onClicked: if (player)
                                        player.shuffle = !player.shuffle
                                }
                                Item {
                                    Layout.fillWidth: true
                                }
                                MediaButton {
                                    iconText: ""
                                    enabled: player?.canGoPrevious ?? false
                                    onClicked: player?.previous()
                                }
                                PlayPauseButton {
                                    isPlaying: player?.playbackState === MprisPlaybackState.Playing
                                    enabled: player?.canTogglePlaying ?? false
                                    onClicked: player?.togglePlaying()
                                }
                                MediaButton {
                                    iconText: ""
                                    enabled: player?.canGoNext ?? false
                                    onClicked: player?.next()
                                }
                                Item {
                                    Layout.fillWidth: true
                                }
                                MediaButton {
                                    iconText: player?.loopState === MprisLoopState.Track ? "" : ""
                                    active: player?.loopState !== MprisLoopState.None
                                    onClicked: {
                                        if (!player)
                                            return;
                                        var ls = player.loopState;
                                        if (ls === MprisLoopState.None)
                                            player.loopState = MprisLoopState.Playlist;
                                        else if (ls === MprisLoopState.Playlist)
                                            player.loopState = MprisLoopState.Track;
                                        else
                                            player.loopState = MprisLoopState.None;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // ─── 汎用メディアボタン ────────────────────────────────────────
    component MediaButton: Rectangle {
        property string iconText: ""
        property bool active: false
        signal clicked

        implicitWidth: 28
        implicitHeight: 28
        radius: DeepSpacePalette.radiusSm
        color: hovered ? DeepSpacePalette.nebulaDim : "transparent"
        opacity: enabled ? 1.0 : 0.3

        property bool hovered: false

        Behavior on color {
            ColorAnimation {
                duration: DeepSpacePalette.durationFast
            }
        }

        Text {
            id: btnIcon
            anchors.centerIn: parent
            text: parent.iconText
            font.pixelSize: 13
            font.family: DeepSpacePalette.fontMono
            color: parent.active ? DeepSpacePalette.nebulaLight : DeepSpacePalette.text
        }

        SequentialAnimation {
            id: btnPressAnim
            NumberAnimation {
                target: btnIcon
                property: "scale"
                to: 0.65
                duration: 70
                easing.type: Easing.OutCubic
            }
            NumberAnimation {
                target: btnIcon
                property: "scale"
                to: 1.0
                duration: 130
                easing.type: Easing.OutBack
            }
        }

        MouseArea {
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onEntered: parent.hovered = true
            onExited: parent.hovered = false
            onPressed: btnPressAnim.start()
            onClicked: parent.clicked()
        }
    }

    // ─── 再生 / 停止ボタン ────────────────────────────────────────
    component PlayPauseButton: Rectangle {
        property bool isPlaying: false
        signal clicked

        implicitWidth: 34
        implicitHeight: 34
        radius: 17
        color: hovered ? DeepSpacePalette.nebulaGlow : DeepSpacePalette.nebulaDim

        property bool hovered: false

        Behavior on color {
            ColorAnimation {
                duration: DeepSpacePalette.durationFast
            }
        }

        Text {
            id: playIcon
            anchors.centerIn: parent
            text: parent.isPlaying ? "󰏤" : "󰐊"
            font.pixelSize: 14
            font.family: DeepSpacePalette.fontMono
            color: DeepSpacePalette.nebulaLight
        }

        SequentialAnimation {
            id: playPressAnim
            NumberAnimation {
                target: playIcon
                property: "scale"
                to: 0.65
                duration: 70
                easing.type: Easing.OutCubic
            }
            NumberAnimation {
                target: playIcon
                property: "scale"
                to: 1.0
                duration: 130
                easing.type: Easing.OutBack
            }
        }

        MouseArea {
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onEntered: parent.hovered = true
            onExited: parent.hovered = false
            onPressed: playPressAnim.start()
            onClicked: parent.clicked()
        }
    }
}
