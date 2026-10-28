// SP-10 helper (dev.cupper.spike.sp10.host), shaped like the product host: the main thread in a
// CFRunLoop, NSApplication only when the system starts it. Modes: notify <session> stay|exit,
// open-main <session> [scheme], status, clear, and no arguments (as the system starts it).
import AppKit
import UserNotifications

let center = UNUserNotificationCenter.current()

func after(_ seconds: Double, _ work: @escaping () -> Void) {
    DispatchQueue.main.asyncAfter(deadline: .now() + seconds, execute: work)
}

func frontmost() -> String { NSWorkspace.shared.frontmostApplication?.bundleIdentifier ?? "-" }
/// After a click: activate the main app, found by bundle id (design 5.2.9); when
/// it is not running, launch it and hand that one app one URL.
func openMain(_ session: String, scheme: String = scheme) {
    let byID = NSWorkspace.shared.urlForApplication(withBundleIdentifier: mainID)
    let outer = Bundle.main.bundleURL.deletingLastPathComponent().deletingLastPathComponent()
        .deletingLastPathComponent()
    log("main app by id=\(byID?.path ?? "none") enclosing=\(outer.path) frontmost=\(frontmost())")
    guard let app = byID else { return }
    let config = NSWorkspace.OpenConfiguration()
    if let running = NSRunningApplication.runningApplications(withBundleIdentifier: mainID).first {
        let asked = running.activate()
        after(1) {
            log("activate() -> \(asked), frontmost=\(frontmost())")
            guard frontmost() != mainID else { return }
            NSWorkspace.shared.openApplication(at: app, configuration: config) { _, error in
                after(1) { log("openApplication error=\(String(describing: error)) frontmost=\(frontmost())") }
            }
        }
    } else {
        let url = URL(string: "\(scheme)://session/\(session)")!
        NSWorkspace.shared.open([url], withApplicationAt: app, configuration: config) { launched, error in
            let pid = launched.map { "\($0.processIdentifier)" } ?? "none"
            log("open \(url) -> pid=\(pid) error=\(String(describing: error))")
            after(2) { log("frontmost=\(frontmost())") }
        }
    }
}

final class Delegate: NSObject, UNUserNotificationCenterDelegate {
    func userNotificationCenter(
        _: UNUserNotificationCenter, willPresent note: UNNotification,
        withCompletionHandler done: @escaping (UNNotificationPresentationOptions) -> Void
    ) {
        log("willPresent \(note.request.identifier)")
        done([.banner, .list])
    }

    func userNotificationCenter(
        _: UNUserNotificationCenter, didReceive response: UNNotificationResponse,
        withCompletionHandler done: @escaping () -> Void
    ) {
        let session = response.notification.request.content.userInfo["session"] as? String ?? "?"
        log("didReceive \(response.notification.request.identifier) action=\(response.actionIdentifier)"
            + " main-thread=\(Thread.isMainThread)")
        done()
        DispatchQueue.main.async {
            openMain(session)
            after(5) { log("exit after the click"); exit(0) }
        }
    }
}

func settings(_ then: @escaping () -> Void) {
    center.getNotificationSettings { s in
        center.getDeliveredNotifications { delivered in
            log("authorization=\(s.authorizationStatus.rawValue) alert=\(s.alertSetting.rawValue)"
                + " style=\(s.alertStyle.rawValue) delivered=\(delivered.map(\.request.identifier))")
            then()
        }
    }
}

let delegate = Delegate()
center.delegate = delegate
let args = Array(CommandLine.arguments.dropFirst())
let service = ProcessInfo.processInfo.environment["XPC_SERVICE_NAME"] ?? "-"
let registered = NSWorkspace.shared.urlForApplication(withBundleIdentifier: hostID)?.path ?? "no"
log("start argv=\(args) ppid=\(getppid()) sid=\(getsid(0)) XPC_SERVICE_NAME=\(service)"
    + " bundle=\(Bundle.main.bundleIdentifier ?? "-") registered=\(registered)")
// usernoted finds its client through LaunchServices, which does not register a nested helper.
if registered == "no" {
    log("LSRegisterURL -> \(LSRegisterURL(Bundle.main.bundleURL as CFURL, false))")
}
var lifetime = 30.0
switch (args.first, args.count) {
case ("notify", 3):
    lifetime = 900
    center.requestAuthorization(options: [.alert, .sound]) { granted, error in
        log("requestAuthorization granted=\(granted) error=\(String(describing: error))")
        let content = UNMutableNotificationContent()
        content.title = "SP-10 \(args[1])"
        content.body = "Click me (helper: \(args[2]))"
        content.userInfo = ["session": args[1]]
        center.add(UNNotificationRequest(identifier: "sp10-\(args[1])", content: content, trigger: nil)) {
            log("posted \(args[1]) error=\(String(describing: $0))")
            settings { if args[2] == "exit" { after(1) { log("exit after posting"); exit(0) } } }
        }
    }
case ("open-main", 2...3):  // after the app that spawned it has quit
    after(2) { openMain(args[1], scheme: args.count == 3 ? args[2] : scheme) }
    lifetime = 7
case ("status", 1), ("clear", 1):
    if args[0] == "clear" { center.removeAllDeliveredNotifications() }
    after(1) { settings { exit(0) } }
default: break
}
// A timer keeps the run loop from returning at once and bounds the helper's life.
Timer.scheduledTimer(withTimeInterval: lifetime, repeats: false) { _ in log("exit: timeout"); exit(0) }
// Started by the system: a bare run loop never got the click (step s2b), so try AppKit's.
if args.isEmpty { NSApplication.shared.run() } else { CFRunLoopRun() }
