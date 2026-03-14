import QtQuick 2.15
import QtQuick.Controls 2.15 as QQC2
import QtQuick.Layouts 1.15

Item {
    id: page

    property string cfg_restaurantCode: "0437"
    property string cfg_enabledRestaurantCodes: "0437,snellari-rss,0436,043601,0439,antell-round,antell-highway,huomen-bioteknia"
    property alias cfg_refreshMinutes: refreshSpin.value
    property int cfg_manualRefreshToken: 0
    property alias cfg_showPrices: showPricesCheck.checked
    property alias cfg_hideExpensiveStudentMeals: hideExpensiveStudentMealsCheck.checked
    property alias cfg_showStudentPrice: showStudentPriceCheck.checked
    property alias cfg_showStaffPrice: showStaffPriceCheck.checked
    property alias cfg_showGuestPrice: showGuestPriceCheck.checked
    property string cfg_iconName: "food"
    property alias cfg_showAllergens: showAllergensCheck.checked
    property alias cfg_highlightGlutenFree: highlightGlutenFreeCheck.checked
    property alias cfg_highlightVeg: highlightVegCheck.checked
    property alias cfg_highlightLactoseFree: highlightLactoseFreeCheck.checked
    property alias cfg_enableWheelCycle: wheelCycleCheck.checked
    property string cfg_lastUpdatedDisplay: ""
    property string cfg_language: "fi"

    property var allRestaurantOptions: [
        { code: "0437", label: "Ita-Suomen yliopisto/Snellmania (0437)", shortLabel: "Snellmania" },
        { code: "snellari-rss", label: "Cafe Snellari (RSS)", shortLabel: "Snellari" },
        { code: "0436", label: "Ita-Suomen yliopisto/Canthia (0436)", shortLabel: "Canthia" },
        { code: "043601", label: "Ita-Suomen yliopisto/Mediteknia (043601)", shortLabel: "Mediteknia" },
        { code: "0439", label: "Tietoteknia (0439)", shortLabel: "Tietoteknia" },
        { code: "antell-round", label: "Antell Round", shortLabel: "Round" },
        { code: "antell-highway", label: "Antell Highway", shortLabel: "Highway" },
        { code: "huomen-bioteknia", label: "Hyvä Huomen Bioteknia (JSON)", shortLabel: "Hyvä Huomen" }
    ]

    function defaultRestaurantCode() {
        return allRestaurantOptions.length > 0 ? allRestaurantOptions[0].code : "0437"
    }

    function canonicalizeCodes(codes) {
        var selectedMap = {}
        var selectedList = Array.isArray(codes) ? codes : []

        for (var i = 0; i < selectedList.length; i++) {
            var code = String(selectedList[i] || "").trim()
            if (code.length > 0) {
                selectedMap[code] = true
            }
        }

        var canonical = []
        for (var j = 0; j < allRestaurantOptions.length; j++) {
            var optionCode = allRestaurantOptions[j].code
            if (selectedMap[optionCode]) {
                canonical.push(optionCode)
            }
        }

        if (canonical.length === 0 && allRestaurantOptions.length > 0) {
            canonical.push(defaultRestaurantCode())
        }

        return canonical
    }

    function parseEnabledCodes(rawValue) {
        var raw = String(rawValue || "").trim()
        if (!raw) {
            var allCodes = []
            for (var i = 0; i < allRestaurantOptions.length; i++) {
                allCodes.push(allRestaurantOptions[i].code)
            }
            return canonicalizeCodes(allCodes)
        }

        return canonicalizeCodes(raw.split(","))
    }

    function enabledCodesList() {
        return parseEnabledCodes(cfg_enabledRestaurantCodes)
    }

    function setEnabledCodes(codes) {
        var canonical = canonicalizeCodes(codes)
        if (canonical.indexOf(cfg_restaurantCode) < 0) {
            canonical.push(cfg_restaurantCode)
            canonical = canonicalizeCodes(canonical)
        }
        cfg_enabledRestaurantCodes = canonical.join(",")
    }

    function optionEnabled(code) {
        return enabledCodesList().indexOf(code) >= 0
    }

    function availableRestaurantOptions() {
        var selected = enabledCodesList()
        var selectedMap = {}
        for (var i = 0; i < selected.length; i++) {
            selectedMap[selected[i]] = true
        }

        var options = []
        for (var j = 0; j < allRestaurantOptions.length; j++) {
            if (selectedMap[allRestaurantOptions[j].code]) {
                options.push(allRestaurantOptions[j])
            }
        }

        if (options.length === 0 && allRestaurantOptions.length > 0) {
            options.push(allRestaurantOptions[0])
        }

        return options
    }

    function restaurantIndexForCode(code) {
        var list = restaurantCombo.model
        for (var i = 0; i < list.length; i++) {
            if (list[i].code === code) {
                return i
            }
        }
        return 0
    }

    function ensureFavoriteInCycle() {
        var selected = enabledCodesList()
        if (selected.indexOf(cfg_restaurantCode) < 0) {
            selected.push(cfg_restaurantCode)
            cfg_enabledRestaurantCodes = canonicalizeCodes(selected).join(",")
        }
    }

    function syncRestaurantCombo() {
        ensureFavoriteInCycle()

        var model = availableRestaurantOptions()
        restaurantCombo.model = model
        if (!model || model.length === 0) {
            return
        }

        var idx = restaurantIndexForCode(cfg_restaurantCode)
        if (restaurantCombo.currentIndex !== idx) {
            restaurantCombo.currentIndex = idx
        }

        if (restaurantCombo.currentIndex < 0 || restaurantCombo.currentIndex >= model.length) {
            restaurantCombo.currentIndex = 0
        }

        cfg_restaurantCode = model[restaurantCombo.currentIndex].code
    }

    function toggleRestaurantEnabled(code, checked) {
        var selected = enabledCodesList()
        var idx = selected.indexOf(code)

        if (code === cfg_restaurantCode) {
            setEnabledCodes(selected)
            return
        }

        if (checked) {
            if (idx < 0) {
                selected.push(code)
            }
        } else {
            if (idx >= 0) {
                selected.splice(idx, 1)
            }
        }

        setEnabledCodes(selected)
        syncRestaurantCombo()
    }

    function syncLanguageCombo() {
        var idx = languageCombo.model.indexOf(cfg_language)
        if (idx < 0) {
            idx = 0
            cfg_language = languageCombo.model[0]
        }
        if (languageCombo.currentIndex !== idx) {
            languageCombo.currentIndex = idx
        }
    }

    function iconIndexForName(name) {
        var list = iconCombo.model
        for (var i = 0; i < list.length; i++) {
            if (list[i].name === name) {
                return i
            }
        }
        return 0
    }

    function syncIconCombo() {
        var idx = iconIndexForName(cfg_iconName)
        if (iconCombo.currentIndex !== idx) {
            iconCombo.currentIndex = idx
        }
        if (iconCombo.currentIndex >= 0) {
            cfg_iconName = iconCombo.model[iconCombo.currentIndex].name
        }
    }

    onCfg_restaurantCodeChanged: {
        ensureFavoriteInCycle()
        syncRestaurantCombo()
    }
    onCfg_enabledRestaurantCodesChanged: syncRestaurantCombo()
    onCfg_languageChanged: syncLanguageCombo()
    onCfg_iconNameChanged: syncIconCombo()

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 12
        spacing: 12

        QQC2.Label {
            text: "Favorite restaurant"
        }

        QQC2.ComboBox {
            id: restaurantCombo
            Layout.fillWidth: true
            textRole: "label"
            model: page.availableRestaurantOptions()
            onCurrentIndexChanged: {
                if (currentIndex >= 0 && model && currentIndex < model.length) {
                    cfg_restaurantCode = model[currentIndex].code
                    ensureFavoriteInCycle()
                }
            }
            Component.onCompleted: page.syncRestaurantCombo()
        }

        QQC2.Label {
            text: "Restaurants in cycle"
        }

        GridLayout {
            Layout.fillWidth: true
            columns: 3
            columnSpacing: 10
            rowSpacing: 4

            Repeater {
                model: page.allRestaurantOptions

                QQC2.CheckBox {
                    text: modelData.shortLabel
                    checked: page.optionEnabled(modelData.code)
                    enabled: modelData.code !== page.cfg_restaurantCode
                    onClicked: page.toggleRestaurantEnabled(modelData.code, checked)
                }
            }
        }

        QQC2.Label {
            text: "Language"
        }

        QQC2.ComboBox {
            id: languageCombo
            Layout.fillWidth: true
            model: ["fi", "en"]
            onCurrentTextChanged: cfg_language = currentText
            Component.onCompleted: page.syncLanguageCombo()
        }

        QQC2.Label {
            text: "Automatic refresh interval (minutes)"
        }

        QQC2.SpinBox {
            id: refreshSpin
            from: 0
            to: 10080
            stepSize: 60
        }

        QQC2.CheckBox {
            id: showPricesCheck
            text: "Show prices"
        }

        QQC2.CheckBox {
            id: hideExpensiveStudentMealsCheck
            text: "Hide Compass meals with student price over 4 €"
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: 10
            enabled: showPricesCheck.checked
            opacity: enabled ? 1.0 : 0.55

            QQC2.Label {
                text: "Price groups"
            }

            QQC2.CheckBox {
                id: showStudentPriceCheck
                text: "Student"
            }

            QQC2.CheckBox {
                id: showStaffPriceCheck
                text: "Staff"
            }

            QQC2.CheckBox {
                id: showGuestPriceCheck
                text: "Guest"
            }
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: 10

            QQC2.Label {
                text: "Tray icon"
            }

            QQC2.ComboBox {
                id: iconCombo
                Layout.fillWidth: true
                textRole: "label"
                model: [
                    { name: "food", label: "Food (default)" },
                    { name: "compass", label: "Compass" },
                    { name: "map-globe", label: "Globe" },
                    { name: "map-flat", label: "Map" }
                ]
                onCurrentIndexChanged: {
                    if (currentIndex >= 0) {
                        cfg_iconName = model[currentIndex].name
                    }
                }
                Component.onCompleted: page.syncIconCombo()
            }
        }

        QQC2.CheckBox {
            id: showAllergensCheck
            text: "Show allergens"
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: 10
            enabled: showAllergensCheck.checked
            opacity: enabled ? 1.0 : 0.55

            QQC2.Label {
                text: "Highlight"
            }

            QQC2.CheckBox {
                id: highlightGlutenFreeCheck
                text: "G"
            }

            QQC2.CheckBox {
                id: highlightVegCheck
                text: "Veg"
            }

            QQC2.CheckBox {
                id: highlightLactoseFreeCheck
                text: "L"
            }
        }

        QQC2.CheckBox {
            id: wheelCycleCheck
            text: "Use mouse wheel on tray icon to switch restaurant"
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: 8

            QQC2.Button {
                text: "Refresh menus now"
                onClicked: cfg_manualRefreshToken = cfg_manualRefreshToken + 1
            }

            QQC2.Button {
                text: "Report issue"
                onClicked: Qt.openUrlExternally("https://github.com/veetir/uef-kuopio-lunch-tray/issues")
            }
        }

        QQC2.Label {
            text: "Last successful update"
        }

        QQC2.Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            text: cfg_lastUpdatedDisplay.length > 0 ? cfg_lastUpdatedDisplay : "No successful update yet"
        }

        Item {
            Layout.fillHeight: true
        }
    }
}
