import AppKit
import SwiftUI

/// What the Swift side asks of the system on a Mac.
enum Platform {
    static let isPhone = false

    static func reveal(_ path: String) {
        NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: path)])
    }

    static func openURL(_ s: String) {
        if let u = URL(string: s) { NSWorkspace.shared.open(u) }
    }

    /// A folder or a file chosen in the system's panel (the mods, the OMSI 2 folder).
    @MainActor
    static func pick(folder: Bool, types: [String] = [], completion: @escaping (URL?) -> Void) {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = folder
        panel.canChooseFiles = !folder
        panel.allowsMultipleSelection = false
        if !types.isEmpty { panel.allowedContentTypes = types.compactMap { .init(filenameExtension: $0) } }
        panel.begin { r in completion(r == .OK ? panel.url : nil) }
    }

    /// A mod chosen in the panel: read where it is (a sandboxed build reads it through the
    /// access the panel gave).
    static func importForInstall(_ url: URL) -> String {
        _ = url.startAccessingSecurityScopedResource()
        return url.path
    }

    /// The OMSI 2 folder chosen in the panel: the access to it is kept for the next starts
    /// (a bookmark; what a sandboxed app needs to read a folder again).
    static func keepAccess(_ url: URL) -> String {
        _ = url.startAccessingSecurityScopedResource()
        if let b = try? url.bookmarkData(options: [.withSecurityScope], includingResourceValuesForKeys: nil, relativeTo: nil) {
            UserDefaults.standard.set(b, forKey: "rootBookmark")
        }
        return url.path
    }

    /// The access to the OMSI 2 folder chosen before (at the start, before the launcher reads it).
    static func restoreAccess() {
        guard let b = UserDefaults.standard.data(forKey: "rootBookmark") else { return }
        var stale = false
        if let url = try? URL(resolvingBookmarkData: b, options: [.withSecurityScope], relativeTo: nil, bookmarkDataIsStale: &stale) {
            _ = url.startAccessingSecurityScopedResource()
            if stale { _ = keepAccess(url) }
        }
    }
}
