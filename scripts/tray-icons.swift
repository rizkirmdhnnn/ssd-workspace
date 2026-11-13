// Renders the menu bar icons in src-tauri/icons/tray/ from SF Symbols.
//
//   swift scripts/tray-icons.swift [point size]   (default 14; 15 is the largest that fits)
//
// tray-icon shows the icon at a fixed height of 18 pt and derives the width from the image's
// aspect ratio. So each image is 36 px tall (18 pt at 2x) with the symbol filling it, and all
// four share one width so the icon does not shift when the state changes. The images are
// black on transparent and used as template images (macOS tints them for light/dark).
import AppKit

let pointSize = CommandLine.arguments.count > 1 ? CGFloat(Double(CommandLine.arguments[1])!) : 14
let scale: CGFloat = 2
let heightPx = 18 * scale
let out = URL(fileURLWithPath: #filePath)
    .deletingLastPathComponent().deletingLastPathComponent()
    .appendingPathComponent("src-tauri/icons/tray")

let states: [(file: String, symbol: String)] = [
    ("tray-mounted", "externaldrive.fill"),
    ("tray-unmounted", "externaldrive.badge.xmark"),
    ("tray-backup", "externaldrive.badge.timemachine"),
    ("tray-ejecting", "eject.circle"),
]

let config = NSImage.SymbolConfiguration(pointSize: pointSize, weight: .regular)
let images = states.map { state -> NSImage in
    guard let img = NSImage(systemSymbolName: state.symbol, accessibilityDescription: nil)?
        .withSymbolConfiguration(config)
    else { fatalError("missing SF Symbol \(state.symbol)") }
    return img
}
// Common canvas width: the widest symbol, rounded up to whole pixels.
let widthPx = ceil(images.map { $0.size.width * scale }.max()!)

for (state, img) in zip(states, images) {
    let rep = NSBitmapImageRep(
        bitmapDataPlanes: nil, pixelsWide: Int(widthPx), pixelsHigh: Int(heightPx),
        bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
        colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
    let w = img.size.width * scale, h = img.size.height * scale
    let rect = NSRect(x: ((widthPx - w) / 2).rounded(), y: ((heightPx - h) / 2).rounded(),
                      width: w, height: h)
    img.draw(in: rect)
    NSColor.black.set()
    rect.fill(using: .sourceAtop)
    NSGraphicsContext.restoreGraphicsState()
    let url = out.appendingPathComponent("\(state.file).png")
    try! rep.representation(using: .png, properties: [:])!.write(to: url)
    print("\(state.file): \(state.symbol) \(Int(w))x\(Int(h)) px in \(Int(widthPx))x\(Int(heightPx))")
}
