import AppKit
import SwiftUI

struct MenuBarReentryTracker {
    private(set) var hasLeftMenuBar = false

    mutating func panelOpened() {
        hasLeftMenuBar = false
    }

    mutating func panelClosed() {
        hasLeftMenuBar = false
    }

    mutating func shouldDismiss(cursorIsInMenuBar: Bool) -> Bool {
        if cursorIsInMenuBar {
            return hasLeftMenuBar
        }
        hasLeftMenuBar = true
        return false
    }
}

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    private let appModel = AppModel.shared
    private let panelState = PanelState()
    private let settingsState = SettingsState(appModel: .shared)
    private let panel = LunchPanel()
    private var statusItem: NSStatusItem?
    private var refreshTimer: Timer?
    private var localEventMonitor: Any?
    private var globalScrollMonitor: Any?
    private var globalClickMonitor: Any?
    private var menuBarMouseMonitor: Any?
    private var workspaceObservers: [NSObjectProtocol] = []
    private var menuBarReentryTracker = MenuBarReentryTracker()
    private var updateCheckTask: Task<Void, Never>?
    private var scrollAccumulator: CGFloat = 0
    private var lastRestaurantScrollAt = Date.distantPast
    private let panelSize = NSSize(width: 440, height: 560)

    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.accessory)
        if ProcessInfo.processInfo.environment["XCTestConfigurationFilePath"] == nil,
           ProcessInfo.processInfo.environment["XCODE_RUNNING_FOR_PREVIEWS"] != "1" {
            appModel.configureLaunchAtLoginIfNeeded()
        }

        let statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength)
        if let button = statusItem.button {
            let image = NSImage(systemSymbolName: "fork.knife", accessibilityDescription: "Lunch Tray")
            image?.isTemplate = true
            button.image = image
            button.toolTip = "Lunch Tray"
            button.target = self
            button.action = #selector(togglePanel)
            button.sendAction(on: [.leftMouseUp, .rightMouseUp])
        }
        self.statusItem = statusItem
        configurePanel()
        installEventMonitors()
        installWorkspaceObservers()

        Task {
            await appModel.prepareMenusInBackground()
        }

        let timer = Timer(timeInterval: 15 * 60, repeats: true) { _ in
            Task { @MainActor in
                await AppModel.shared.prepareMenusInBackground()
            }
        }
        RunLoop.main.add(timer, forMode: .common)
        refreshTimer = timer
    }

    @objc private func togglePanel() {
        guard let button = statusItem?.button else { return }

        if NSApp.currentEvent?.type == .rightMouseUp {
            showContextMenu()
            return
        }

        if panel.isVisible {
            hidePanel()
        } else {
            positionPanel(below: button)
            button.highlight(true)
            startMenuBarReentryTracking()
            panel.makeKeyAndOrderFront(nil)
            Task {
                await appModel.refreshIfNeeded()
            }
        }
    }

    private func hidePanel() {
        stopMenuBarReentryTracking()
        panel.orderOut(nil)
        statusItem?.button?.highlight(false)
        panelState.isShowingSettings = false
    }

    func applicationWillTerminate(_ notification: Notification) {
        refreshTimer?.invalidate()
        updateCheckTask?.cancel()
        if let localEventMonitor {
            NSEvent.removeMonitor(localEventMonitor)
        }
        if let globalScrollMonitor {
            NSEvent.removeMonitor(globalScrollMonitor)
        }
        if let globalClickMonitor {
            NSEvent.removeMonitor(globalClickMonitor)
        }
        stopMenuBarReentryTracking()
        let workspaceNotificationCenter = NSWorkspace.shared.notificationCenter
        for observer in workspaceObservers {
            workspaceNotificationCenter.removeObserver(observer)
        }
        workspaceObservers.removeAll()
    }

    @objc private func terminateApp() {
        NSApp.terminate(nil)
    }

    @objc private func checkForUpdates() {
        guard updateCheckTask == nil else { return }
        let currentVersion = Bundle.main.object(
            forInfoDictionaryKey: "CFBundleShortVersionString"
        ) as? String ?? "0.0.0"

        updateCheckTask = Task { [weak self] in
            guard let self else { return }
            defer { updateCheckTask = nil }
            do {
                let result = try await MacUpdateChecker().check(
                    currentVersion: currentVersion
                )
                guard !Task.isCancelled else { return }
                showUpdateResult(result)
            } catch is CancellationError {
                return
            } catch {
                guard !Task.isCancelled else { return }
                showUpdateError(error)
            }
        }
    }

    private func showContextMenu() {
        hidePanel()
        guard let statusItem, let button = statusItem.button else { return }

        let menu = NSMenu()
        let updateTitle = appModel.language == .fi
            ? "Tarkista päivitykset…"
            : "Check for Updates…"
        let updateItem = NSMenuItem(
            title: updateTitle,
            action: #selector(checkForUpdates),
            keyEquivalent: ""
        )
        updateItem.target = self
        updateItem.isEnabled = updateCheckTask == nil
        menu.addItem(updateItem)
        menu.addItem(.separator())

        let quitTitle = appModel.language == .fi ? "Lopeta" : "Quit"
        let quitItem = NSMenuItem(
            title: quitTitle,
            action: #selector(terminateApp),
            keyEquivalent: "q"
        )
        quitItem.target = self
        menu.addItem(quitItem)

        statusItem.menu = menu
        button.performClick(nil)
        statusItem.menu = nil
    }

    private func showUpdateResult(_ result: MacUpdateCheckResult) {
        let finnish = appModel.language == .fi
        let alert = NSAlert()
        alert.alertStyle = .informational

        switch result {
        case let .latestPublished(currentVersion, _):
            alert.messageText = finnish
                ? "Lunch Tray on ajan tasalla"
                : "Lunch Tray is up to date"
            alert.informativeText = finnish
                ? "Versio \(currentVersion) on uusin julkaistu versio."
                : "Version \(currentVersion) is the latest published version."
            alert.addButton(withTitle: "OK")
        case let .updateAvailable(
            currentVersion,
            latestVersion,
            releaseURL
        ):
            alert.messageText = finnish
                ? "Päivitys saatavilla"
                : "Update available"
            alert.informativeText = finnish
                ? "Versio \(latestVersion) on saatavilla. Käytössä on versio \(currentVersion)."
                : "Version \(latestVersion) is available. You are using \(currentVersion)."
            alert.addButton(withTitle: finnish ? "Avaa julkaisu" : "Open Release")
            alert.addButton(withTitle: finnish ? "Peruuta" : "Cancel")
            if alert.runModal() == .alertFirstButtonReturn {
                NSWorkspace.shared.open(releaseURL)
            }
            return
        case let .newerThanLatestPublished(currentVersion, latestVersion):
            alert.messageText = finnish
                ? "Käytössä on julkaistua uudempi versio"
                : "You are using a newer version"
            alert.informativeText = finnish
                ? "Käytössä on versio \(currentVersion). Uusin julkaistu versio on \(latestVersion)."
                : "You are using \(currentVersion). The latest published version is \(latestVersion)."
            alert.addButton(withTitle: "OK")
        }

        alert.runModal()
    }

    private func showUpdateError(_ error: Error) {
        let finnish = appModel.language == .fi
        let alert = NSAlert()
        alert.alertStyle = .warning
        alert.messageText = finnish
            ? "Päivityksiä ei voitu tarkistaa"
            : "Could not check for updates"
        alert.informativeText = error.localizedDescription
        alert.addButton(withTitle: "OK")
        alert.runModal()
    }

    private func installEventMonitors() {
        localEventMonitor = NSEvent.addLocalMonitorForEvents(
            matching: [
                .keyDown,
                .scrollWheel,
                .leftMouseDown,
                .rightMouseDown,
                .mouseMoved
            ]
        ) { [weak self] event in
            guard let self else { return event }
            if event.type == .mouseMoved {
                self.handleMenuBarReentry()
                return event
            }
            if event.type == .keyDown, self.handleKeyDown(event) {
                return nil
            }
            if event.type == .scrollWheel {
                if self.handleRestaurantScroll(event) {
                    return nil
                }
            }
            if event.type == .leftMouseDown || event.type == .rightMouseDown {
                self.hidePanelForOutsideClick()
            }
            return event
        }

        globalScrollMonitor = NSEvent.addGlobalMonitorForEvents(
            matching: .scrollWheel
        ) { [weak self] event in
            Task { @MainActor in
                _ = self?.handleRestaurantScroll(event)
            }
        }

        globalClickMonitor = NSEvent.addGlobalMonitorForEvents(
            matching: [.leftMouseDown, .rightMouseDown]
        ) { [weak self] _ in
            Task { @MainActor in
                self?.hidePanel()
            }
        }
    }

    private func installWorkspaceObservers() {
        let notificationCenter = NSWorkspace.shared.notificationCenter
        let activeSpaceObserver = notificationCenter.addObserver(
            forName: NSWorkspace.activeSpaceDidChangeNotification,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            Task { @MainActor in
                self?.hidePanel()
            }
        }
        workspaceObservers.append(activeSpaceObserver)
    }

    private func startMenuBarReentryTracking() {
        menuBarReentryTracker.panelOpened()
        guard menuBarMouseMonitor == nil else { return }
        menuBarMouseMonitor = NSEvent.addGlobalMonitorForEvents(
            matching: .mouseMoved
        ) { [weak self] _ in
            Task { @MainActor in
                self?.handleMenuBarReentry()
            }
        }
    }

    private func stopMenuBarReentryTracking() {
        menuBarReentryTracker.panelClosed()
        if let menuBarMouseMonitor {
            NSEvent.removeMonitor(menuBarMouseMonitor)
            self.menuBarMouseMonitor = nil
        }
    }

    private func handleMenuBarReentry() {
        guard panel.isVisible else { return }
        if menuBarReentryTracker.shouldDismiss(
            cursorIsInMenuBar: cursorIsInMenuBarArea()
        ) {
            hidePanel()
        }
    }

    private func cursorIsInMenuBarArea() -> Bool {
        let point = NSEvent.mouseLocation
        guard let screen = screen(containing: point) else { return false }
        let menuBarHeight = max(
            NSStatusBar.system.thickness + 4,
            screen.frame.maxY - screen.visibleFrame.maxY + 4
        )
        return point.y >= screen.frame.maxY - menuBarHeight
    }

    private func screen(containing point: NSPoint) -> NSScreen? {
        NSScreen.screens.first { screen in
            point.x >= screen.frame.minX
                && point.x < screen.frame.maxX
                && point.y >= screen.frame.minY
                && point.y <= screen.frame.maxY
        }
    }

    private func handleKeyDown(_ event: NSEvent) -> Bool {
        guard panel.isVisible else { return false }

        if panelState.isShowingSettings {
            if event.keyCode == 53 {
                panelState.isShowingSettings = false
                return true
            }
            return false
        }

        let blockedModifiers: NSEvent.ModifierFlags = [.command, .control, .option]
        guard event.modifierFlags.intersection(blockedModifiers).isEmpty else {
            return false
        }

        switch event.keyCode {
        case 53:
            hidePanel()
            return true
        case 123:
            appModel.selectPreviousRestaurant()
            return true
        case 124:
            appModel.selectNextRestaurant()
            return true
        case 125:
            return panel.scrollMenuByItem(direction: 1)
        case 126:
            return panel.scrollMenuByItem(direction: -1)
        case 116:
            return panel.scrollMenu(by: -panel.visibleMenuHeight * 0.85)
        case 121:
            return panel.scrollMenu(by: panel.visibleMenuHeight * 0.85)
        default:
            break
        }

        guard let character = event.charactersIgnoringModifiers?.first,
              let number = character.wholeNumberValue
        else {
            return false
        }
        return appModel.selectRestaurant(shortcutNumber: number)
    }

    private func handleRestaurantScroll(_ event: NSEvent) -> Bool {
        guard !panelState.isShowingSettings else { return false }
        guard cursorIsOverStatusItem() else {
            scrollAccumulator = 0
            return false
        }
        guard event.momentumPhase.isEmpty else { return true }

        if event.phase.contains(.began) {
            scrollAccumulator = 0
        }

        let delta = abs(event.scrollingDeltaX) > abs(event.scrollingDeltaY)
            ? event.scrollingDeltaX
            : event.scrollingDeltaY
        scrollAccumulator += delta

        let threshold: CGFloat = event.hasPreciseScrollingDeltas ? 18 : 0.5
        let now = Date()
        guard abs(scrollAccumulator) >= threshold,
              now.timeIntervalSince(lastRestaurantScrollAt) >= 0.18
        else {
            return true
        }

        if scrollAccumulator < 0 {
            appModel.selectNextRestaurant()
        } else {
            appModel.selectPreviousRestaurant()
        }
        scrollAccumulator = 0
        lastRestaurantScrollAt = now
        return true
    }

    private func cursorIsOverStatusItem() -> Bool {
        statusItemScreenRect()?.insetBy(dx: -2, dy: -2).contains(NSEvent.mouseLocation) ?? false
    }

    private func configurePanel() {
        panelState.onDismissPanel = { [weak self] in
            self?.hidePanel()
        }
        panel.contentViewController = NSHostingController(
            rootView: MenuPopoverView()
                .environmentObject(appModel)
                .environmentObject(panelState)
                .environmentObject(settingsState)
        )
        panel.setContentSize(panelSize)
        panel.styleMask = [.borderless, .nonactivatingPanel]
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.hasShadow = true
        panel.isFloatingPanel = true
        panel.hidesOnDeactivate = false
        panel.acceptsMouseMovedEvents = true
        panel.level = .popUpMenu
        panel.collectionBehavior = [.moveToActiveSpace, .fullScreenAuxiliary, .transient]
        panel.contentViewController?.view.layoutSubtreeIfNeeded()
    }

    private func positionPanel(below button: NSStatusBarButton) {
        guard let buttonRect = statusItemScreenRect(),
              let screen = button.window?.screen ?? NSScreen.main
        else {
            return
        }

        let screenFrame = screen.frame
        let horizontalMargin: CGFloat = 8
        let desiredX = buttonRect.midX - panelSize.width / 2
        let maximumX = screenFrame.maxX - panelSize.width - horizontalMargin
        let x = min(max(desiredX, screenFrame.minX + horizontalMargin), maximumX)
        let desiredTop = buttonRect.minY - 5
        let y = max(screenFrame.minY + 8, desiredTop - panelSize.height)

        panel.setFrame(
            NSRect(origin: NSPoint(x: x, y: y), size: panelSize),
            display: true
        )
    }

    private func hidePanelForOutsideClick() {
        guard panel.isVisible,
              !panel.frame.contains(NSEvent.mouseLocation),
              !cursorIsOverStatusItem()
        else {
            return
        }
        hidePanel()
    }

    private func statusItemScreenRect() -> NSRect? {
        guard let button = statusItem?.button, let window = button.window else {
            return nil
        }
        let buttonRectInWindow = button.convert(button.bounds, to: nil)
        return window.convertToScreen(buttonRectInWindow)
    }
}

private final class LunchPanel: NSPanel {
    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { false }

    var visibleMenuHeight: CGFloat {
        menuScrollView?.contentView.bounds.height ?? 0
    }

    func scrollMenuByItem(direction: Int) -> Bool {
        guard direction != 0,
              let scrollView = menuScrollView,
              let documentView = scrollView.documentView
        else {
            return false
        }

        scrollView.layoutSubtreeIfNeeded()
        documentView.layoutSubtreeIfNeeded()

        let anchors = menuItemAnchors(in: documentView)
        guard !anchors.isEmpty else {
            return scrollMenu(by: CGFloat(direction) * 64)
        }

        let clipView = scrollView.contentView
        let maximumOffset = maximumVisualOffset(
            documentView: documentView,
            clipView: clipView
        )
        guard maximumOffset > 1 else { return true }

        let currentOffset = visualOffset(
            documentView: documentView,
            clipView: clipView
        )
        let itemOffsets = anchors.map {
            itemTopOffset(
                rect: $0,
                documentView: documentView
            )
        }
        let targetOffset = MenuItemScrollNavigator.targetOffset(
            currentOffset: currentOffset,
            direction: direction,
            itemTopOffsets: itemOffsets,
            maximumOffset: maximumOffset,
            tailSnapThreshold: min(96, clipView.bounds.height * 0.2)
        )

        scroll(
            clipView,
            in: scrollView,
            documentView: documentView,
            toVisualOffset: targetOffset
        )
        return true
    }

    func scrollMenu(by visualDelta: CGFloat) -> Bool {
        guard let scrollView = menuScrollView,
              let documentView = scrollView.documentView
        else {
            return false
        }

        scrollView.layoutSubtreeIfNeeded()
        let clipView = scrollView.contentView
        let maximumOffset = maximumVisualOffset(
            documentView: documentView,
            clipView: clipView
        )
        guard maximumOffset > 1 else { return true }

        let currentOffset = visualOffset(
            documentView: documentView,
            clipView: clipView
        )
        let targetOffset = min(
            max(currentOffset + visualDelta, 0),
            maximumOffset
        )
        scroll(
            clipView,
            in: scrollView,
            documentView: documentView,
            toVisualOffset: targetOffset
        )
        return true
    }

    private func menuItemAnchors(in documentView: NSView) -> [NSRect] {
        descendantMenuItemAnchors(in: documentView)
            .map { view in
                view.convert(view.bounds, to: documentView)
            }
            .sorted {
                itemTopOffset(rect: $0, documentView: documentView)
                    < itemTopOffset(rect: $1, documentView: documentView)
            }
    }

    private func descendantMenuItemAnchors(
        in view: NSView
    ) -> [MenuItemScrollAnchorView] {
        var anchors: [MenuItemScrollAnchorView] = []
        if let anchor = view as? MenuItemScrollAnchorView {
            anchors.append(anchor)
        }
        for subview in view.subviews {
            anchors.append(contentsOf: descendantMenuItemAnchors(in: subview))
        }
        return anchors
    }

    private func visualOffset(
        documentView: NSView,
        clipView: NSClipView
    ) -> CGFloat {
        if documentView.isFlipped {
            return clipView.bounds.minY - documentView.bounds.minY
        }
        return documentView.bounds.maxY - clipView.bounds.maxY
    }

    private func itemTopOffset(
        rect: NSRect,
        documentView: NSView
    ) -> CGFloat {
        if documentView.isFlipped {
            return rect.minY - documentView.bounds.minY
        }
        return documentView.bounds.maxY - rect.maxY
    }

    private func maximumVisualOffset(
        documentView: NSView,
        clipView: NSClipView
    ) -> CGFloat {
        max(0, documentView.bounds.height - clipView.bounds.height)
    }

    private func scroll(
        _ clipView: NSClipView,
        in scrollView: NSScrollView,
        documentView: NSView,
        toVisualOffset targetOffset: CGFloat
    ) {
        let targetY = documentView.isFlipped
            ? documentView.bounds.minY + targetOffset
            : documentView.bounds.maxY - clipView.bounds.height - targetOffset
        clipView.scroll(
            to: NSPoint(x: clipView.bounds.origin.x, y: targetY)
        )
        scrollView.reflectScrolledClipView(clipView)
    }

    private var menuScrollView: NSScrollView? {
        guard let contentView else { return nil }
        return MenuScrollViewFinder.find(in: contentView)
    }
}

enum MenuItemScrollNavigator {
    static func targetOffset(
        currentOffset: CGFloat,
        direction: Int,
        itemTopOffsets: [CGFloat],
        maximumOffset: CGFloat,
        tailSnapThreshold: CGFloat
    ) -> CGFloat {
        guard direction != 0, maximumOffset > 0 else {
            return min(max(currentOffset, 0), maximumOffset)
        }

        var stops = [CGFloat(0)]
        stops.append(contentsOf: itemTopOffsets.map {
            min(max($0, 0), maximumOffset)
        })
        stops.append(maximumOffset)
        stops.sort()

        let tolerance: CGFloat = 1
        stops = stops.reduce(into: []) { result, offset in
            if let previous = result.last,
               abs(previous - offset) <= tolerance {
                return
            }
            result.append(offset)
        }

        if direction > 0,
           stops.count >= 3,
           let tailStart = stops.dropLast().last,
           tailStart > tolerance,
           maximumOffset - tailStart <= tailSnapThreshold {
            stops.remove(at: stops.count - 2)
        }

        if direction > 0 {
            return stops.first(where: { $0 > currentOffset + tolerance })
                ?? maximumOffset
        }
        return stops.last(where: { $0 < currentOffset - tolerance }) ?? 0
    }
}

enum MenuScrollViewFinder {
    static func find(in view: NSView) -> NSScrollView? {
        if let scrollView = view as? NSScrollView,
           let documentView = scrollView.documentView,
           containsMenuScrollMarker(in: documentView) {
            return scrollView
        }
        for subview in view.subviews {
            if let scrollView = find(in: subview) {
                return scrollView
            }
        }
        return nil
    }

    private static func containsMenuScrollMarker(in view: NSView) -> Bool {
        if view is MenuScrollViewMarkerView {
            return true
        }
        return view.subviews.contains(where: containsMenuScrollMarker)
    }
}
