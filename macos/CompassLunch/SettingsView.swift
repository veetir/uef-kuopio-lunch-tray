import SwiftUI

struct SettingsView: View {
    @EnvironmentObject private var appModel: AppModel

    var body: some View {
        Form {
            Section("Menu") {
                Picker("Default restaurant", selection: $appModel.selectedRestaurantCode) {
                    ForEach(appModel.restaurants) { restaurant in
                        Text(restaurant.name).tag(restaurant.id)
                    }
                }

                Picker("Language", selection: $appModel.language) {
                    ForEach(AppLanguage.allCases) { language in
                        Text(language.title).tag(language)
                    }
                }
            }

            Section("Display") {
                Picker("Lunch item layout", selection: $appModel.lunchLayout) {
                    ForEach(LunchLayout.allCases) { layout in
                        Text(layout.title).tag(layout)
                    }
                }
                .pickerStyle(.segmented)

                Toggle("Show prices", isOn: $appModel.showPrices)
                Toggle("Student prices", isOn: $appModel.showStudentPrice)
                    .disabled(!appModel.showPrices)
                Toggle("Staff prices", isOn: $appModel.showStaffPrice)
                    .disabled(!appModel.showPrices)
                Toggle("Guest prices", isOn: $appModel.showGuestPrice)
                    .disabled(!appModel.showPrices)
                Toggle("Show allergens and diets", isOn: $appModel.showAllergens)
                Toggle("Show CO₂ emissions", isOn: $appModel.showCarbonEmissions)
            }

            Section("Highlights") {
                HighlightEditor(
                    title: "Meals",
                    placeholder: "Meal name or text",
                    values: appModel.highlightedMeals,
                    add: appModel.addMealHighlight,
                    remove: appModel.removeMealHighlight
                )

                Divider()

                HighlightEditor(
                    title: "Ingredients",
                    placeholder: "Ingredient or text",
                    values: appModel.highlightedIngredients,
                    add: appModel.addIngredientHighlight,
                    remove: appModel.removeIngredientHighlight
                )
            }
        }
        .formStyle(.grouped)
        .frame(width: 480, height: 620)
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
