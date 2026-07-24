import AppKit

@main
enum CompassLunchApp {
    @MainActor
    private static let appDelegate = AppDelegate()

    @MainActor
    static func main() {
        let application = NSApplication.shared
        application.delegate = appDelegate
        application.run()
    }
}
