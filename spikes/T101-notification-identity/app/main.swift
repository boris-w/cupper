// SP-10 main app (dev.cupper.spike.sp10): a window listing the URLs it was
// handed; it launches the helper as the product UI launches its host.
//   spawn <helper arguments>       spawn the helper and stay
//   spawn-quit <helper arguments>  spawn the helper and quit, as when the UI is gone
import AppKit

/// posix_spawn in a session of its own with only fds 0-2 (on /dev/null) and an
/// allow-listed environment, as `cupper-sys::spawn`; never through LaunchServices.
func spawnHost(_ args: [String]) {
    let exe = Bundle.main.bundlePath + "/Contents/Helpers/SP10Host.app/Contents/MacOS/sp10-host"
    var attr: posix_spawnattr_t?
    posix_spawnattr_init(&attr)
    posix_spawnattr_setflags(&attr, Int16(POSIX_SPAWN_SETSID | POSIX_SPAWN_CLOEXEC_DEFAULT))
    var files: posix_spawn_file_actions_t?
    posix_spawn_file_actions_init(&files)
    for fd: Int32 in 0...2 {
        posix_spawn_file_actions_addopen(&files, fd, "/dev/null", fd == 0 ? O_RDONLY : O_WRONLY, 0)
    }
    let env: [String] = ["HOME=\(NSHomeDirectory())", "PATH=/usr/bin:/bin"]
    let argv = ([exe] + args).map { strdup($0) } + [nil]
    let envp = env.map { strdup($0) } + [nil]
    var pid: pid_t = 0
    let rc = posix_spawn(&pid, exe, &files, &attr, argv, envp)
    log("spawned helper rc=\(rc) pid=\(pid)")
}

final class App: NSObject, NSApplicationDelegate {
    let label = NSTextField(labelWithString: "SP10 Spike: the main app\n")
    var window: NSWindow?

    func applicationDidFinishLaunching(_: Notification) {
        let args = Array(CommandLine.arguments.dropFirst())
        log("start argv=\(args) ppid=\(getppid())")
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 460, height: 140), styleMask: [.titled, .closable],
            backing: .buffered, defer: false)
        window.title = "SP10 Spike"
        window.contentView = label
        window.center()
        window.makeKeyAndOrderFront(nil)
        self.window = window
        if args.first == "spawn" || args.first == "spawn-quit" { spawnHost(Array(args.dropFirst())) }
        if args.first == "spawn-quit" { NSApp.terminate(nil) }
    }

    func application(_: NSApplication, open urls: [URL]) {
        for url in urls {
            log("opened \(url)")
            label.stringValue += "opened \(url)\n"
        }
    }

    func applicationDidBecomeActive(_: Notification) { log("became active") }
    func applicationShouldTerminateAfterLastWindowClosed(_: NSApplication) -> Bool { true }
}

signal(SIGCHLD, SIG_IGN)  // reap the helper
let delegate = App()
NSApplication.shared.delegate = delegate
NSApplication.shared.run()
