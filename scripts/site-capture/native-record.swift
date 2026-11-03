import AppKit
import ApplicationServices
import AVFoundation
import CoreGraphics
import Foundation

func emit(_ value: Any) throws {
    let data = try JSONSerialization.data(withJSONObject: value, options: [.prettyPrinted, .sortedKeys])
    FileHandle.standardOutput.write(data)
    FileHandle.standardOutput.write(Data("\n".utf8))
}

func displays() -> [[String: Any]] {
    NSScreen.screens.enumerated().map { index, screen in
        let number = screen.deviceDescription[NSDeviceDescriptionKey("NSScreenNumber")] as? NSNumber
        let id = CGDirectDisplayID(number?.uint32Value ?? 0)
        let bounds = CGDisplayBounds(id)
        let mode = CGDisplayCopyDisplayMode(id)
        return [
            "index": index + 1,
            "id": id,
            "name": screen.localizedName,
            "main": id == CGMainDisplayID(),
            // CGDisplayPixelsWide/High can report the desktop size in HiDPI mode.
            // Read the backing pixel dimensions without guessing from the scale factor.
            "pixelWidth": mode?.pixelWidth ?? 0,
            "pixelHeight": mode?.pixelHeight ?? 0,
            "logicalWidth": mode?.width ?? 0,
            "logicalHeight": mode?.height ?? 0,
            "displayModeAvailable": mode != nil,
            "scale": screen.backingScaleFactor,
            "boundsPoints": [bounds.origin.x, bounds.origin.y, bounds.width, bounds.height],
        ]
    }
}

func inspect(_ path: String) throws {
    let url = URL(fileURLWithPath: path)
    let asset = AVURLAsset(url: url)
    guard let track = asset.tracks(withMediaType: .video).first else {
        throw NSError(domain: "native-record", code: 1, userInfo: [NSLocalizedDescriptionKey: "No video track in \(path)"])
    }
    let transformed = CGRect(origin: .zero, size: track.naturalSize).applying(track.preferredTransform)
    let attributes = try FileManager.default.attributesOfItem(atPath: path)
    try emit([
        "path": url.path,
        "width": Int(abs(transformed.width).rounded()),
        "height": Int(abs(transformed.height).rounded()),
        "duration": CMTimeGetSeconds(asset.duration),
        "fps": track.nominalFrameRate,
        "bytes": attributes[.size] ?? 0,
        "audioTracks": asset.tracks(withMediaType: .audio).count,
    ])
}

func inspectImage(_ path: String) throws {
    let url = URL(fileURLWithPath: path)
    let data = try Data(contentsOf: url)
    guard let bitmap = NSBitmapImageRep(data: data) else {
        throw NSError(domain: "native-record", code: 3, userInfo: [NSLocalizedDescriptionKey: "No image in \(path)"])
    }
    try emit([
        "path": url.path,
        "width": bitmap.pixelsWide,
        "height": bitmap.pixelsHigh,
        "bytes": data.count,
    ])
}

do {
    let arguments = CommandLine.arguments
    switch arguments.dropFirst().first {
    case "preflight":
        try emit([
            "screenRecordingAllowed": CGPreflightScreenCaptureAccess(),
            "accessibilityAllowed": AXIsProcessTrusted(),
            "displays": displays(),
        ])
    case "inspect" where arguments.count == 3:
        try inspect(arguments[2])
    case "image" where arguments.count == 3:
        try inspectImage(arguments[2])
    default:
        throw NSError(domain: "native-record", code: 2, userInfo: [NSLocalizedDescriptionKey: "Use preflight, inspect FILE, or image FILE"])
    }
} catch {
    FileHandle.standardError.write(Data("native-record: \(error.localizedDescription)\n".utf8))
    exit(1)
}
