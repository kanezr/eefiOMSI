import AppKit
import SwiftUI

/// openOMSI on a Mac: this app is the launcher (SwiftUI); the game is the
/// `openomsi` program beside it in the bundle, which the launcher starts as a process of its
/// own for every drive, as on the other computers. With "Cross-Platform Launcher" on (the
/// launcher's Settings → General) the launcher drawn by the game opens instead.
@main
struct OpenOMSIApp: App {
    @State private var store = LauncherStore()
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var delegate

    init() {
        AppLanguage.apply(home: FileManager.default.homeDirectoryForCurrentUser)
        Theme.registerFonts()
    }

    var body: some Scene {
        WindowGroup("openOMSI") {
            LauncherRoot()
                .environment(store)
                .frame(minWidth: 1080, minHeight: 680)
                .preferredColorScheme(.dark)
                .onAppear {
                    Platform.restoreAccess()
                    store.open()
                    if store.flag("cross_platform_launcher") { CrossPlatform.open() }
                }
        }
        .windowStyle(.hiddenTitleBar)
        .windowToolbarStyle(.unifiedCompact)
        .defaultSize(width: 1440, height: 880)
        .commands {
            CommandGroup(replacing: .newItem) {}
            CommandGroup(after: .appInfo) {
                Button("Open the Cross-Platform Launcher") { CrossPlatform.open() }
            }
        }
    }
}

final class AppDelegate: NSObject, NSApplicationDelegate {
    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { true }
}
