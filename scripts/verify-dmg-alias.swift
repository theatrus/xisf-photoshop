// Check the background alias through macOS, without Finder or mount dialogs.
import Foundation
import CoreFoundation

do {
    guard CommandLine.arguments.count == 3 else {
        throw NSError(domain: "DMG", code: 1, userInfo: [
            NSLocalizedDescriptionKey: "usage: verify-dmg-alias.swift <alias record> <expected image>"
        ])
    }
    let record = try Data(contentsOf: URL(fileURLWithPath: CommandLine.arguments[1]))
    guard let bookmark = CFURLCreateBookmarkDataFromAliasRecord(nil, record as CFData) else {
        throw NSError(domain: "DMG", code: 2, userInfo: [
            NSLocalizedDescriptionKey: "macOS could not read the background alias"
        ])
    }
    var stale = false
    let resolved = try URL(
        resolvingBookmarkData: bookmark.takeRetainedValue() as Data,
        options: [.withoutUI, .withoutMounting], relativeTo: nil,
        bookmarkDataIsStale: &stale
    ).resolvingSymlinksInPath()
    let expected = URL(fileURLWithPath: CommandLine.arguments[2]).resolvingSymlinksInPath()
    guard resolved == expected, try resolved.checkResourceIsReachable() else {
        throw NSError(domain: "DMG", code: 3, userInfo: [
            NSLocalizedDescriptionKey: "Background alias resolved to \(resolved.path), expected \(expected.path)"
        ])
    }
    print("macOS resolved the background alias to the image on this volume.")
} catch {
    FileHandle.standardError.write(Data("Background alias check failed: \(error)\n".utf8))
    exit(1)
}
