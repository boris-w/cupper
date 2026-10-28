// Shared by the main app and the helper of the SP-10 prototype (T101).
import Foundation

let mainID = "dev.cupper.spike.sp10"
let hostID = "dev.cupper.spike.sp10.host"
let scheme = "sp10-spike"
let stamp = ISO8601DateFormatter()

/// One line per event; both processes append to the same file, so the order shows.
func log(_ text: String) {
    let line = "\(stamp.string(from: Date())) \(ProcessInfo.processInfo.processName)[\(getpid())] \(text)\n"
    let url = URL(fileURLWithPath: NSHomeDirectory() + "/Library/Logs/dev.cupper.spike.sp10.log")
    guard let file = try? FileHandle(forWritingTo: url) else {
        return try! line.write(to: url, atomically: false, encoding: .utf8)
    }
    file.seekToEndOfFile()
    file.write(Data(line.utf8))
    try? file.close()
}
