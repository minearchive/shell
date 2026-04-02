import QtQuick
import Quickshell.Io
import Quickshell.Services.UPower
import Quickshell.Services.Pipewire
import "../assets"

Item {
    id: root

    // ── CPU state ────────────────────────────────────────────────
    property real cpuPercent: 0
    property var prevCpu: null

    // ── RAM state ────────────────────────────────────────────────
    property real ramUsed: 0   // GB
    property real ramTotal: 0   // GB

    // ── Refresh ──────────────────────────────────────────────────
    Timer {
        interval: 2000
        running: true
        repeat: true
        triggeredOnStart: true
        onTriggered: {
            cpuProc.running = true;
            memProc.running = true;
        }
    }

    // ── /proc/stat → CPU % ───────────────────────────────────────
    Process {
        id: cpuProc
        command: ["awk", "/^cpu /{print $2,$3,$4,$5,$6,$7,$8,$9; exit}", "/proc/stat"]
        stdout: SplitParser {
            onRead: line => {
                var p = line.trim().split(" ");
                var user = parseInt(p[0]);
                var nice = parseInt(p[1]);
                var system = parseInt(p[2]);
                var idle = parseInt(p[3]);
                var iowait = parseInt(p[4]);
                var irq = parseInt(p[5]);
                var softirq = parseInt(p[6]);
                var steal = parseInt(p[7]) || 0;
                var total = user + nice + system + idle + iowait + irq + softirq + steal;

                if (root.prevCpu !== null) {
                    var dt = total - root.prevCpu.total;
                    var di = idle - root.prevCpu.idle;
                    if (dt > 0)
                        root.cpuPercent = Math.round((1 - di / dt) * 100);
                }
                root.prevCpu = {
                    total: total,
                    idle: idle
                };
            }
        }
    }

    // ── /proc/meminfo → RAM ──────────────────────────────────────
    Process {
        id: memProc
        command: ["awk", "/^MemTotal:/{t=$2} /^MemAvailable:/{a=$2} END{print t,a}", "/proc/meminfo"]
        stdout: SplitParser {
            onRead: line => {
                var p = line.trim().split(" ");
                var total = parseInt(p[0]);   // kB
                var avail = parseInt(p[1]);   // kB
                root.ramTotal = Math.round(total / 1048576 * 10) / 10;
                root.ramUsed = Math.round((total - avail) / 1048576 * 10) / 10;
            }
        }
    }

    // ── Computed display values ──────────────────────────────────
    readonly property string volText: {
        if (Pipewire.defaultAudioSink == null)
            return "—";
        if (Pipewire.defaultAudioSink.audio.muted || Pipewire.defaultAudioSink.audio.volume === 0)
            return "muted";
        return Math.round(Pipewire.defaultAudioSink.audio.volume * 100) + "%";
    }
    readonly property string batText: {
        var pct = Math.floor(UPower.displayDevice.percentage * 100);
        if (UPower.displayDevice.state === UPowerDeviceState.Charging)
            return pct + "% ↑";
        return pct + "%";
    }
    readonly property color batColor: {
        var p = UPower.displayDevice.percentage;
        if (p < 0.20)
            return DeepSpacePalette.mars;
        if (p < 0.50)
            return DeepSpacePalette.marsLight;
        return DeepSpacePalette.auroraLight;
    }
    readonly property string ramText: ramUsed + " / " + ramTotal + " G"
    readonly property string cpuText: cpuPercent + "%"
    readonly property color cpuColor: {
        if (cpuPercent > 80)
            return DeepSpacePalette.mars;
        if (cpuPercent > 50)
            return DeepSpacePalette.marsLight;
        return DeepSpacePalette.oceanLight;
    }

    // ── Layout ───────────────────────────────────────────────────
    Column {
        anchors {
            top: parent.top
            left: parent.left
            right: parent.right
        }
        spacing: 0

        // Section title
        Text {
            text: "SYSTEM"
            font.pixelSize: 9
            font.family: DeepSpacePalette.fontMono
            font.weight: DeepSpacePalette.fontWeightMedium
            color: DeepSpacePalette.textDim
            font.letterSpacing: 2
            bottomPadding: 16
        }

        // Metric rows
        Repeater {
            model: [
                {
                    label: "VOL",
                    value: root.volText,
                    color: DeepSpacePalette.oceanLight
                },
                {
                    label: "BAT",
                    value: root.batText,
                    color: root.batColor
                },
                {
                    label: "RAM",
                    value: root.ramText,
                    color: DeepSpacePalette.auroraLight
                },
                {
                    label: "CPU",
                    value: root.cpuText,
                    color: root.cpuColor
                },
            ]

            Column {
                width: parent.width
                spacing: 2
                topPadding: 10
                bottomPadding: 10

                Text {
                    text: modelData.label
                    font.pixelSize: 9
                    font.family: DeepSpacePalette.fontMono
                    font.weight: DeepSpacePalette.fontWeightMedium
                    color: DeepSpacePalette.textDim
                    font.letterSpacing: 1
                }
                Text {
                    text: modelData.value
                    font.pixelSize: 18
                    font.family: DeepSpacePalette.fontMono
                    color: modelData.color
                }

                // Separator (skip after last item)
                Rectangle {
                    width: parent.width
                    height: 1
                    color: DeepSpacePalette.borderMid
                    opacity: 0.4
                    visible: index < 3
                }
            }
        }
    }
}
