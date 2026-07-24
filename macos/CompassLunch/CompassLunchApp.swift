import SwiftUI

@main
struct CompassLunchApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var appDelegate
    @StateObject private var appModel = AppModel.shared

    var body: some Scene {
        Settings {
            SettingsView()
                .environmentObject(appModel)
        }
    }
}
