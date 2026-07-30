import Combine
import SwiftUI

@MainActor
final class SettingsState: ObservableObject {
    private let appModel: AppModel
    private var appModelObservation: AnyCancellable?

    init(appModel: AppModel) {
        self.appModel = appModel
        appModelObservation = appModel.objectWillChange.sink { [weak self] _ in
            self?.objectWillChange.send()
        }
    }

    var restaurants: [Restaurant] { appModel.restaurants }
    var language: AppLanguage { appModel.language }
    var launchAtLogin: Bool { appModel.launchAtLogin }
    var launchAtLoginRequiresApproval: Bool {
        appModel.launchAtLoginRequiresApproval
    }
    var showPrices: Bool { appModel.showPrices }
    var highlightedMeals: [String] { appModel.highlightedMeals }
    var highlightedIngredients: [String] { appModel.highlightedIngredients }

    func binding<Value>(
        _ keyPath: ReferenceWritableKeyPath<AppModel, Value>
    ) -> Binding<Value> {
        Binding(
            get: { [appModel] in appModel[keyPath: keyPath] },
            set: { [weak self] value in
                guard let self else { return }
                objectWillChange.send()
                appModel[keyPath: keyPath] = value
            }
        )
    }

    func setLaunchAtLogin(_ enabled: Bool) {
        objectWillChange.send()
        appModel.setLaunchAtLogin(enabled)
    }

    func openLoginItemsSettings() {
        appModel.openLoginItemsSettings()
    }

    func addMealHighlight(_ value: String) {
        objectWillChange.send()
        appModel.addMealHighlight(value)
    }

    func removeMealHighlight(_ value: String) {
        objectWillChange.send()
        appModel.removeMealHighlight(value)
    }

    func addIngredientHighlight(_ value: String) {
        objectWillChange.send()
        appModel.addIngredientHighlight(value)
    }

    func removeIngredientHighlight(_ value: String) {
        objectWillChange.send()
        appModel.removeIngredientHighlight(value)
    }

    func refresh() {
        objectWillChange.send()
        appModel.refreshLaunchAtLoginStatus()
    }
}

struct EmbeddedSettingsView: View {
    var body: some View {
        SettingsForm()
            .formStyle(.grouped)
            .scrollContentBackground(.hidden)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

private struct SettingsForm: View {
    @EnvironmentObject private var settings: SettingsState

    var body: some View {
        Form {
            Section("General") {
                Picker(
                    "Default restaurant",
                    selection: settings.binding(\.selectedRestaurantCode)
                ) {
                    ForEach(settings.restaurants) { restaurant in
                        Text(restaurant.name).tag(restaurant.id)
                    }
                }

                Picker(
                    "Language",
                    selection: settings.binding(\.language)
                ) {
                    ForEach(AppLanguage.allCases) { language in
                        Text(language.title).tag(language)
                    }
                }

                Toggle(
                    "Launch at login",
                    isOn: Binding(
                        get: { settings.launchAtLogin },
                        set: settings.setLaunchAtLogin
                    )
                )

                if settings.launchAtLoginRequiresApproval {
                    Button("Open Login Items Settings…") {
                        settings.openLoginItemsSettings()
                    }
                }
            }

            Section("Display") {
                Picker(
                    "Lunch item layout",
                    selection: settings.binding(\.lunchLayout)
                ) {
                    ForEach(LunchLayout.allCases) { layout in
                        Text(layout.title).tag(layout)
                    }
                }
                .pickerStyle(.segmented)

                Picker("Accent", selection: settings.binding(\.accent)) {
                    ForEach(AppAccent.allCases) { accent in
                        Text(accent.title).tag(accent)
                    }
                }
                .pickerStyle(.segmented)

                Toggle(
                    "Show prices",
                    isOn: settings.binding(\.showPrices)
                )
                Toggle(
                    "Student prices",
                    isOn: settings.binding(\.showStudentPrice)
                )
                .disabled(!settings.showPrices)
                Toggle(
                    "Staff prices",
                    isOn: settings.binding(\.showStaffPrice)
                )
                .disabled(!settings.showPrices)
                Toggle(
                    "Guest prices",
                    isOn: settings.binding(\.showGuestPrice)
                )
                .disabled(!settings.showPrices)
                Toggle(
                    "Show allergens and diets",
                    isOn: settings.binding(\.showAllergens)
                )
                Toggle(
                    "Show CO₂ emissions",
                    isOn: settings.binding(\.showCarbonEmissions)
                )
            }

            Section("Highlights") {
                HighlightEditor(
                    title: "Meals",
                    placeholder: "Meal name or text",
                    values: settings.highlightedMeals,
                    add: settings.addMealHighlight,
                    remove: settings.removeMealHighlight
                )

                Divider()

                HighlightEditor(
                    title: "Ingredients",
                    placeholder: "Ingredient or text",
                    values: settings.highlightedIngredients,
                    add: settings.addIngredientHighlight,
                    remove: settings.removeIngredientHighlight
                )
            }
        }
        .onAppear {
            settings.refresh()
        }
    }
}

private struct HighlightEditor: View {
    let title: String
    let placeholder: String
    let values: [String]
    let add: (String) -> Void
    let remove: (String) -> Void

    @State private var draft = ""

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(title)
                .font(.headline)

            HStack {
                TextField(placeholder, text: $draft)
                    .textFieldStyle(.roundedBorder)
                    .onSubmit(addDraft)

                Button("Add") {
                    addDraft()
                }
                .disabled(draft.normalizedWhitespace.isEmpty)
            }

            ForEach(values, id: \.self) { value in
                HStack {
                    Text(value)
                        .lineLimit(1)
                        .help(value)

                    Spacer()

                    Button {
                        remove(value)
                    } label: {
                        Image(systemName: "minus.circle")
                    }
                    .buttonStyle(.borderless)
                    .help("Remove")
                }
            }
        }
        .padding(.vertical, 3)
    }

    private func addDraft() {
        let value = draft.normalizedWhitespace
        guard !value.isEmpty else { return }
        add(value)
        draft = ""
    }
}
