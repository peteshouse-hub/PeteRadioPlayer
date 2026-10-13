import AppKit
import Foundation
import Darwin

/// Native AppKit shell. The proven Rust FLAC/HTTP transport runs unchanged in a child process.
final class RadioKingDelegate: NSObject, NSApplicationDelegate {
    private var server: Process?
    private let playerURL = URL(string: "http://127.0.0.1:8765/")!
    private var quitting = false

    func applicationDidFinishLaunching(_ notification: Notification) {
        let executable = Bundle.main.resourceURL!.appendingPathComponent("RadioKingServer")
        let process = Process()
        process.executableURL = executable
        var environment = ProcessInfo.processInfo.environment
        environment["RADIOKING_NATIVE_LAUNCHER"] = "1"
        process.environment = environment
        process.terminationHandler = { [weak self] ended in
            DispatchQueue.main.async {
                guard let self = self, !self.quitting else { return }
                let alert = NSAlert()
                alert.messageText = "RadioKing Live server stopped"
                alert.informativeText = "The audio server exited (status \(ended.terminationStatus)). Check that port 8765 is free."
                alert.runModal()
                NSApp.terminate(nil)
            }
        }
        do {
            try process.run()
            server = process
        } catch {
            let alert = NSAlert()
            alert.messageText = "Cannot start RadioKing Live"
            alert.informativeText = error.localizedDescription
            alert.runModal()
            NSApp.terminate(nil)
            return
        }
        // Allow the HTTP listener to start before opening the browser.
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.9) { [weak self] in
            guard let self = self, !self.quitting, process.isRunning else { return }
            self.showPlayer()
        }
    }

    private func showPlayer() {
        NSWorkspace.shared.open(playerURL)
    }

    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
        showPlayer() // Dock click returns to the existing browser-based interface.
        return true
    }

    func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
        quitting = true
        if let process = server, process.isRunning {
            // Rust's ctrlc handler performs HQPlayer stop/cleanup on SIGINT.
            kill(process.processIdentifier, SIGINT)
        }
        return .terminateNow
    }
}

let app = NSApplication.shared
app.setActivationPolicy(.regular)
let delegate = RadioKingDelegate()
app.delegate = delegate
app.run()
