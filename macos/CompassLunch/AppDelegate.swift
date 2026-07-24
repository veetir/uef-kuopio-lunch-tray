import AppKit
import SwiftUI

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

        Task {
            await appModel.refreshIfNeeded()
        }

        let timer = Timer(timeInterval: 15 * 60, repeats: true) { _ in
            Task { @MainActor in
                await AppModel.shared.refreshIfNeeded()
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
            panel.makeKeyAndOrderFront(nil)
            Task {
                await appModel.refreshIfNeeded()
            }
        }
    }

    private func hidePanel() {
        panel.orderOut(nil)
        statusItem?.button?.highlight(false)
        panelState.isShowingSettings = false
    }

    func applicationWillTerminate(_ notification: Notification) {
        refreshTimer?.invalidate()
        if let localEventMonitor {
            NSEvent.removeMonitor(localEventMonitor)
        }
        if let globalScrollMonitor {
            NSEvent.removeMonitor(globalScrollMonitor)
        }
        if let globalClickMonitor {
            NSEvent.removeMonitor(globalClickMonitor)
        }
    }

    @objc private func terminateApp() {
        NSApp.terminate(nil)
    }

    private func showContextMenu() {
        hidePanel()
        guard let statusItem, let button = statusItem.button else { return }

        let menu = NSMenu()
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

    private func installEventMonitors() {
        localEventMonitor = NSEvent.addLocalMonitorForEvents(
            matching: [.keyDown, .scrollWheel, .leftMouseDown, .rightMouseDown]
        ) { [weak self] event in
            guard let self else { return event }
            if event.type == .keyDown, self.handleKeyDown(event) {
                return nil
            }
            if event.type == .scrollWheel {
                if self.handleRestaurantScroll(event) {
                    return nil
                }
                self.panel.resetMenuItemNavigation()
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
        panel.level = .popUpMenu
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .transient]
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
    private var currentMenuItemID: String?
    private var lastItemScrollOrigin: CGFloat?

    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { false }

    var visibleMenuHeight: CGFloat {
        menuScrollView?.contentView.bounds.height ?? 0
    }

    func resetMenuItemNavigation() {
        currentMenuItemID = nil
        lastItemScrollOrigin = nil
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
        let currentOrigin = clipView.bounds.origin.y
        let maximumY = maximumScrollOrigin(
            documentView: documentView,
            clipView: clipView
        )
        let currentIndex: Int

        if let currentMenuItemID,
           let lastItemScrollOrigin,
           abs(currentOrigin - lastItemScrollOrigin) < 1,
           let trackedIndex = anchors.firstIndex(where: {
               $0.view.anchorID == currentMenuItemID
           }) {
            currentIndex = trackedIndex
        } else {
            currentIndex = inferredMenuItemIndex(
                anchors: anchors,
                documentView: documentView,
                clipView: clipView
            )
        }

        if direction < 0, currentIndex == 0 {
            let minimumY = documentView.bounds.minY
            guard abs(currentOrigin - minimumY) > 1 else { return true }
            scroll(
                clipView,
                in: scrollView,
                toY: minimumY
            )
            currentMenuItemID = anchors[0].view.anchorID
            lastItemScrollOrigin = clipView.bounds.origin.y
            return true
        }

        if direction > 0, currentIndex == anchors.count - 1 {
            guard abs(currentOrigin - maximumY) > 1 else { return true }
            scroll(
                clipView,
                in: scrollView,
                toY: maximumY
            )
            currentMenuItemID = anchors[currentIndex].view.anchorID
            lastItemScrollOrigin = clipView.bounds.origin.y
            return true
        }

        let targetIndex = min(
            max(currentIndex + direction, 0),
            anchors.count - 1
        )
        let target = anchors[targetIndex]
        let unclampedY = documentView.isFlipped
            ? target.rect.minY
            : target.rect.maxY - clipView.bounds.height
        let targetY = min(
            max(unclampedY, documentView.bounds.minY),
            maximumY
        )

        scroll(clipView, in: scrollView, toY: targetY)
        currentMenuItemID = target.view.anchorID
        lastItemScrollOrigin = clipView.bounds.origin.y
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
        let minimumY = documentView.bounds.minY
        let maximumY = maximumScrollOrigin(
            documentView: documentView,
            clipView: clipView
        )
        guard maximumY - minimumY > 1 else { return true }

        let coordinateDelta = documentView.isFlipped ? visualDelta : -visualDelta
        let targetY = min(
            max(clipView.bounds.origin.y + coordinateDelta, minimumY),
            maximumY
        )
        scroll(clipView, in: scrollView, toY: targetY)
        return true
    }

    private typealias MenuItemAnchor = (
        view: MenuItemScrollAnchorView,
        rect: NSRect
    )

    private func menuItemAnchors(in documentView: NSView) -> [MenuItemAnchor] {
        descendantMenuItemAnchors(in: documentView)
            .map { view in
                (
                    view: view,
                    rect: view.convert(view.bounds, to: documentView)
                )
            }
            .sorted { lhs, rhs in
                if documentView.isFlipped {
                    return lhs.rect.minY < rhs.rect.minY
                }
                return lhs.rect.maxY > rhs.rect.maxY
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

    private func inferredMenuItemIndex(
        anchors: [MenuItemAnchor],
        documentView: NSView,
        clipView: NSClipView
    ) -> Int {
        let visibleTop = documentView.isFlipped
            ? clipView.bounds.minY
            : clipView.bounds.maxY
        let tolerance: CGFloat = 2
        var index = 0

        for (candidateIndex, anchor) in anchors.enumerated() {
            let anchorTop = documentView.isFlipped
                ? anchor.rect.minY
                : anchor.rect.maxY
            let isAtOrAboveVisibleTop = documentView.isFlipped
                ? anchorTop <= visibleTop + tolerance
                : anchorTop >= visibleTop - tolerance
            if isAtOrAboveVisibleTop {
                index = candidateIndex
            } else {
                break
            }
        }
        return index
    }

    private func maximumScrollOrigin(
        documentView: NSView,
        clipView: NSClipView
    ) -> CGFloat {
        max(
            documentView.bounds.minY,
            documentView.bounds.maxY - clipView.bounds.height
        )
    }

    private func scroll(
        _ clipView: NSClipView,
        in scrollView: NSScrollView,
        toY targetY: CGFloat
    ) {
        clipView.scroll(
            to: NSPoint(x: clipView.bounds.origin.x, y: targetY)
        )
        scrollView.reflectScrolledClipView(clipView)
    }

    private var menuScrollView: NSScrollView? {
        guard let contentView else { return nil }
        return firstScrollView(in: contentView)
    }

    private func firstScrollView(in view: NSView) -> NSScrollView? {
        if let scrollView = view as? NSScrollView {
            return scrollView
        }
        for subview in view.subviews {
            if let scrollView = firstScrollView(in: subview) {
                return scrollView
            }
        }
        return nil
    }
}
