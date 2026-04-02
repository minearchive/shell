import QtQuick
import Quickshell
import Quickshell.Services.UPower
import Quickshell.Services.Pipewire
import "../assets"

Item {
    implicitWidth: label.implicitWidth
    implicitHeight: label.implicitHeight

    Scope {
        PwObjectTracker {
            objects: [Pipewire.defaultAudioSink]
        }
    }

    Text {
        id: label
        anchors.centerIn: parent
        text: {
            var text = "";
            if (Pipewire.defaultAudioSink == null) {
                text = text + "NaN";
            } else if (Pipewire.defaultAudioSink.audio.muted) {
                text = text + "Muted";
            } else {
                text = text + Math.floor(Pipewire.defaultAudioSink.audio.volume / 1.39 * 100);
            }

            text = text + " | ";

            text = text + Math.floor(UPower.displayDevice.percentage * 100);

            if (UPower.displayDevice.state == UPowerDeviceState.Charging) {
                text = text + "(" + Math.floor(UPower.displayDevice.timeToFull / 60 * 10) / 10 + "m)";
            }

            return text;
        }
        color: DeepSpacePalette.oceanLight

        font {
            pixelSize: 16
            family: DeepSpacePalette.fontMono
        }
    }
}
