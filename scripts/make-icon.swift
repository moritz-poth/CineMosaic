// Draws the app icon for every size on Apple's icon grid, in Mountpoint's style (small sizes have less margin).
//   swift scripts/make-icon.swift            → assets/AppIcon.iconset (all sizes, used by scripts/build-mac-app.sh) + assets/icon.png
//   swift scripts/make-icon.swift <dir>      → only the iconset sizes into <dir>
// Plate copied from MediaFiles Check / Mountpoint. Motif "Raster": 3 × 3 stills, the middle one lit (concept 01,
// chosen by the user 2026-11-21 from 20 drafts). Geometry on the 824 px plate.
import AppKit

/// Plate size per pixel size, measured from macOS system app icons (same table as Mountpoint).
let plateFor: [Int: Int] = [16: 14, 32: 28, 64: 52, 128: 104, 256: 206, 512: 412, 1024: 824]

func color(_ hex: UInt32, _ a: CGFloat = 1) -> CGColor {
    CGColor(red: CGFloat(hex >> 16 & 0xff) / 255, green: CGFloat(hex >> 8 & 0xff) / 255, blue: CGFloat(hex & 0xff) / 255, alpha: a)
}

func render(_ size: Int) -> Data {
    let plateSize = CGFloat(plateFor[size]!)
    let margin = (CGFloat(size) - plateSize) / 2
    let k = plateSize / 824   // everything is designed for the 824 px plate
    let small = size <= 32

    let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: size, pixelsHigh: size, bitsPerSample: 8,
                               samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB,
                               bytesPerRow: 0, bitsPerPixel: 0)!
    NSGraphicsContext.saveGraphicsState()
    let ctx = NSGraphicsContext(bitmapImageRep: rep)!
    NSGraphicsContext.current = ctx
    let cg = ctx.cgContext
    cg.translateBy(x: 0, y: CGFloat(size))   // origin top left
    cg.scaleBy(x: 1, y: -1)

    // plate – identical to Mountpoint
    let plateRect = CGRect(x: margin, y: margin, width: plateSize, height: plateSize)
    let radius = 185 * k
    let plate = CGPath(roundedRect: plateRect, cornerWidth: radius, cornerHeight: radius, transform: nil)
    cg.saveGState()
    if size >= 64 { cg.setShadow(offset: CGSize(width: 0, height: -10 * k), blur: 20 * k, color: color(0x000000, 0.35)) }
    cg.addPath(plate)
    cg.setFillColor(color(0x2e3236))
    cg.fillPath()
    cg.restoreGState()
    cg.saveGState()
    cg.addPath(plate)
    cg.clip()
    let gradient = CGGradient(colorsSpace: CGColorSpaceCreateDeviceRGB(),
                              colors: [color(0x3c4146), color(0x272a2e)] as CFArray, locations: [0, 1])!
    cg.drawLinearGradient(gradient, start: CGPoint(x: 0, y: plateRect.minY), end: CGPoint(x: 0, y: plateRect.maxY), options: [])
    cg.restoreGState()
    let edge = max(3 * k, 0.75)
    cg.addPath(CGPath(roundedRect: plateRect.insetBy(dx: edge / 2, dy: edge / 2),
                      cornerWidth: radius - edge / 2, cornerHeight: radius - edge / 2, transform: nil))
    cg.setStrokeColor(color(0xffffff, 0.10))
    cg.setLineWidth(edge)
    cg.strokePath()

    // motif "Raster" (user's pick 2026-11-21): 3 × 3 stills, the middle one lit amber = the middle frame of every shot.
    // Tile 168 + gap 24 on the 824 plate; at 16/32 px whole pixels (3+1, 6+2) so the gaps stay open.
    let tints: [UInt32] = [0x3b6275, 0xc9743f, 0x5e7f6b, 0x8a4e5c, 0xf2b33d, 0x6c8ea0, 0xa85e3a, 0x2f4858, 0xd9c9a8]
    let (tile, gap, corner): (CGFloat, CGFloat, CGFloat) =
        size <= 16 ? (3, 1, 0.5) : size <= 32 ? (6, 2, 1) : (168 * k, 24 * k, 20 * k)
    let block = 3 * tile + 2 * gap
    let x0 = small ? ((CGFloat(size) - block) / 2).rounded(.down) : (CGFloat(size) - block) / 2
    for i in 0..<9 {
        let f = CGRect(x: x0 + CGFloat(i % 3) * (tile + gap), y: x0 + CGFloat(i / 3) * (tile + gap), width: tile, height: tile)
        cg.addPath(CGPath(roundedRect: f, cornerWidth: corner, cornerHeight: corner, transform: nil))
        cg.setFillColor(color(tints[i]))
        cg.fillPath()
    }

    NSGraphicsContext.restoreGraphicsState()
    return rep.representation(using: .png, properties: [:])!
}

func writeIconset(_ dir: String) {
    try! FileManager.default.createDirectory(atPath: dir, withIntermediateDirectories: true)
    for s in [16, 32, 128, 256, 512] {
        try! render(s).write(to: URL(fileURLWithPath: "\(dir)/icon_\(s)x\(s).png"))
        try! render(s * 2).write(to: URL(fileURLWithPath: "\(dir)/icon_\(s)x\(s)@2x.png"))
    }
}

if let dir = CommandLine.arguments.dropFirst().first {
    writeIconset(dir)
    print(dir)
} else {
    writeIconset("assets/AppIcon.iconset")
    try! render(1024).write(to: URL(fileURLWithPath: "assets/icon.png"))
    print("assets/AppIcon.iconset, assets/icon.png")
}
