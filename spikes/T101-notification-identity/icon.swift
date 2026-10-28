// A 1024 px icon: one white letter on a rounded square. icon.swift <letter> <r,g,b> <out.png>
import AppKit

let a = CommandLine.arguments
let rgb = a[2].split(separator: ",").map { CGFloat(Double($0)!) }
let rep = NSBitmapImageRep(
    bitmapDataPlanes: nil, pixelsWide: 1024, pixelsHigh: 1024, bitsPerSample: 8, samplesPerPixel: 4,
    hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
NSColor(red: rgb[0], green: rgb[1], blue: rgb[2], alpha: 1).setFill()
NSBezierPath(roundedRect: NSRect(x: 100, y: 100, width: 824, height: 824), xRadius: 180, yRadius: 180).fill()
let letter = NSAttributedString(
    string: a[1], attributes: [.font: NSFont.boldSystemFont(ofSize: 560), .foregroundColor: NSColor.white])
letter.draw(at: NSPoint(x: 512 - letter.size().width / 2, y: 512 - letter.size().height / 2))
try! rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: a[3]))
