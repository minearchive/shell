import QtQuick
import "../assets"
import "../singleton"

Item {
    id: root

    implicitHeight: col.implicitHeight

    // ── State ────────────────────────────────────────────────────
    property int displayYear: {
        var d = new Date();
        return d.getFullYear();
    }
    property int displayMonth: {
        var d = new Date();
        return d.getMonth();   // 0–11
    }

    // ── Helpers ──────────────────────────────────────────────────
    readonly property var monthNames: [
        "January","February","March","April","May","June",
        "July","August","September","October","November","December"
    ]
    readonly property int daysInMonth: new Date(displayYear, displayMonth + 1, 0).getDate()
    readonly property int firstWeekday: new Date(displayYear, displayMonth, 1).getDay()   // 0=Sun
    readonly property int todayYear:  { var d = new Date(); return d.getFullYear(); }
    readonly property int todayMonth: { var d = new Date(); return d.getMonth(); }
    readonly property int todayDay:   { var d = new Date(); return d.getDate(); }

    // Number of leading blank cells + day cells, padded to full weeks
    readonly property int gridCells: Math.ceil((firstWeekday + daysInMonth) / 7) * 7

    // ── Layout ───────────────────────────────────────────────────
    Column {
        id: col
        width: parent.width
        spacing: 10

        // ── Month / Year header ──────────────────────────────────
        Item {
            width: parent.width
            implicitHeight: 28

            // Previous month
            Text {
                id: prevBtn
                anchors { left: parent.left; verticalCenter: parent.verticalCenter }
                text: "‹"
                font.pixelSize: 16
                font.family: DeepSpacePalette.fontMono
                color: hovered ? DeepSpacePalette.nebulaLight : DeepSpacePalette.text
                property bool hovered: false

                HoverHandler { onHoveredChanged: prevBtn.hovered = hovered }
                TapHandler {
                    onTapped: {
                        if (root.displayMonth === 0) {
                            root.displayMonth = 11;
                            root.displayYear -= 1;
                        } else {
                            root.displayMonth -= 1;
                        }
                    }
                }
            }

            // Month + year label
            Text {
                anchors.centerIn: parent
                text: root.monthNames[root.displayMonth] + "  " + root.displayYear
                font.pixelSize: 13
                font.family: DeepSpacePalette.fontMono
                font.weight: DeepSpacePalette.fontWeightMedium
                color: DeepSpacePalette.star
            }

            // Next month
            Text {
                id: nextBtn
                anchors { right: parent.right; verticalCenter: parent.verticalCenter }
                text: "›"
                font.pixelSize: 16
                font.family: DeepSpacePalette.fontMono
                color: hovered ? DeepSpacePalette.nebulaLight : DeepSpacePalette.text
                property bool hovered: false

                HoverHandler { onHoveredChanged: nextBtn.hovered = hovered }
                TapHandler {
                    onTapped: {
                        if (root.displayMonth === 11) {
                            root.displayMonth = 0;
                            root.displayYear += 1;
                        } else {
                            root.displayMonth += 1;
                        }
                    }
                }
            }
        }

        // ── Day-of-week headers ──────────────────────────────────
        Row {
            width: parent.width
            spacing: 0

            Repeater {
                model: ["Su","Mo","Tu","We","Th","Fr","Sa"]

                Text {
                    width: root.width / 7
                    horizontalAlignment: Text.AlignHCenter
                    text: modelData
                    font.pixelSize: 11
                    font.family: DeepSpacePalette.fontMono
                    color: index === 0 || index === 6
                        ? DeepSpacePalette.nebula
                        : DeepSpacePalette.textDim
                }
            }
        }

        // ── Thin separator ───────────────────────────────────────
        Rectangle {
            width: parent.width
            height: 1
            color: DeepSpacePalette.borderMid
            opacity: 0.5
        }

        // ── Day grid ─────────────────────────────────────────────
        Grid {
            columns: 7
            width: parent.width
            spacing: 0

            Repeater {
                model: root.gridCells

                delegate: Item {
                    id: cell
                    width: root.width / 7
                    height: 38

                    // day = 1-based date; 0 means empty (padding)
                    readonly property int day: {
                        var d = index - root.firstWeekday + 1;
                        return (d >= 1 && d <= root.daysInMonth) ? d : 0;
                    }
                    readonly property bool isToday:
                        day > 0 &&
                        root.displayYear  === root.todayYear  &&
                        root.displayMonth === root.todayMonth &&
                        day               === root.todayDay
                    readonly property bool isWeekend: (index % 7 === 0 || index % 7 === 6)
                    property bool hovered: false

                    // Today highlight circle
                    Rectangle {
                        anchors.centerIn: parent
                        width: 30; height: 30
                        radius: 15
                        color: cell.isToday ? DeepSpacePalette.nebulaDim : "transparent"
                        border.color: cell.isToday ? DeepSpacePalette.nebula : "transparent"
                        border.width: 1
                    }

                    // Hover highlight
                    Rectangle {
                        anchors.centerIn: parent
                        width: 30; height: 30
                        radius: 15
                        color: cell.hovered && cell.day > 0 && !cell.isToday
                            ? DeepSpacePalette.surface2 : "transparent"
                    }

                    Text {
                        anchors.centerIn: parent
                        text: cell.day > 0 ? cell.day : ""
                        font.pixelSize: 13
                        font.family: DeepSpacePalette.fontMono
                        font.weight: cell.isToday
                            ? DeepSpacePalette.fontWeightMedium
                            : DeepSpacePalette.fontWeightNormal
                        color: {
                            if (cell.isToday)    return DeepSpacePalette.nebulaLight;
                            if (cell.isWeekend)  return DeepSpacePalette.textDim;
                            return DeepSpacePalette.text;
                        }
                    }

                    HoverHandler { onHoveredChanged: cell.hovered = hovered }
                }
            }
        }
    }
}
