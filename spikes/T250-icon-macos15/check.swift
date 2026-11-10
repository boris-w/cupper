// T250: the latte icon as macOS draws it beside its own apps (design 5.10.1).
// Makes a probe app with the icns and asks NSWorkspace for the icons, as
// Finder and the Dock get them; writes results/compare.png (one row at 256 px,
// one at 64 px: the probe, then system apps) and results/info.txt.
//   swift check.swift <icon.icns>
import AppKit

let files = FileManager.default
let results = URL(fileURLWithPath: "results", isDirectory: true)
let probe = results.appendingPathComponent("Cupper Probe.app")
try? files.removeItem(at: probe)
let resources = probe.appendingPathComponent("Contents/Resources")
try! files.createDirectory(at: resources, withIntermediateDirectories: true)
try! files.copyItem(
    atPath: CommandLine.arguments[1], toPath: resources.appendingPathComponent("icon.icns").path)
// An app with no executable gets Finder's "prohibited" badge (first run).
let macos = probe.appendingPathComponent("Contents/MacOS")
try! files.createDirectory(at: macos, withIntermediateDirectories: true)
try! files.copyItem(atPath: "/usr/bin/true", toPath: macos.appendingPathComponent("probe").path)
let plist: NSDictionary = [
    "CFBundleIdentifier": "dev.cupper.spike.icon", "CFBundleName": "Cupper Probe",
    "CFBundleIconFile": "icon", "CFBundlePackageType": "APPL", "CFBundleExecutable": "probe",
]
plist.write(to: probe.appendingPathComponent("Contents/Info.plist"), atomically: true)

let apps = [
    probe.path, "/System/Applications/Calculator.app", "/System/Applications/Notes.app",
    "/System/Applications/TextEdit.app", "/System/Applications/Utilities/Terminal.app",
]
let (large, small) = (256, 64)
let rep = NSBitmapImageRep(
    bitmapDataPlanes: nil, pixelsWide: large * apps.count, pixelsHigh: large + small,
    bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
    colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
for (index, path) in apps.enumerated() {
    let icon = NSWorkspace.shared.icon(forFile: path)
    for (side, y) in [(large, small), (small, 0)] {
        icon.draw(in: NSRect(x: index * large + (large - side) / 2, y: y, width: side, height: side))
    }
}
NSGraphicsContext.current = nil
try! rep.representation(using: .png, properties: [:])!.write(
    to: results.appendingPathComponent("compare.png"))
let system = ProcessInfo.processInfo.operatingSystemVersionString
try! "macOS \(system)\nleft to right: \(apps.map { ($0 as NSString).lastPathComponent })\n"
    .write(to: results.appendingPathComponent("info.txt"), atomically: true, encoding: .utf8)
print(try! String(contentsOf: results.appendingPathComponent("info.txt"), encoding: .utf8))
