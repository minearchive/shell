// qmllint disable import
import QtQuick
import "../assets"
import "../singleton"

Text {
    // qmllint disable missing-property
    text: Time.time
    font.pixelSize: 18
    font.family: DeepSpacePalette.fontMono
    color: DeepSpacePalette.star
}
