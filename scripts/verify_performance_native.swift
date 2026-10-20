// Repeatable native scroll/idle workload for a dedicated verification instance.
// Captures come from the app's renderer; no Screen Recording permission is used.
import AppKit
import ApplicationServices
import Foundation

func option(_ name: String, _ fallback: String = "") -> String {
    CommandLine.arguments.first { $0.hasPrefix("--\(name)=") }?
        .dropFirst(name.count + 3).description ?? fallback
}
let grants = ["accessibility": AXIsProcessTrusted(), "postEvents": CGPreflightPostEventAccess()]
if CommandLine.arguments.contains("--check-permissions") {
    print(String(data: try JSONSerialization.data(withJSONObject: grants), encoding: .utf8)!)
    exit(grants.values.allSatisfy { $0 } ? 0 : 2)
}
guard grants.values.allSatisfy({ $0 }) else { fatalError("Missing Accessibility/input permission; no consent requested") }
let name = option("app")
guard !name.isEmpty, let app = NSWorkspace.shared.runningApplications.first(where: { $0.localizedName == name }) else {
    fatalError("Pass --app= with the exact name of an already running dedicated instance")
}
let output = URL(fileURLWithPath: option("output", "artifacts/performance-native"))
let capture = URL(fileURLWithPath: option("capture"))
guard option("capture").hasPrefix("/") else { fatalError("Pass app-owned --capture=/absolute/frame.png") }
try FileManager.default.createDirectory(at: output, withIntermediateDirectories: true)
let root = AXUIElementCreateApplication(app.processIdentifier)
func value(_ element: AXUIElement, _ key: String) -> CFTypeRef? {
    var result: CFTypeRef?
    AXUIElementCopyAttributeValue(element, key as CFString, &result)
    return result
}
func rect(_ element: AXUIElement) -> CGRect {
    var origin = CGPoint.zero, size = CGSize.zero
    if let v = value(element, kAXPositionAttribute) { AXValueGetValue(v as! AXValue, .cgPoint, &origin) }
    if let v = value(element, kAXSizeAttribute) { AXValueGetValue(v as! AXValue, .cgSize, &size) }
    return CGRect(origin: origin, size: size)
}
guard let window = (value(root, kAXWindowsAttribute) as? [AXUIElement])?.first(where: {
    value($0, kAXSubroleAttribute) as? String == kAXStandardWindowSubrole
}) else { fatalError("No standard verification window") }
let bounds = rect(window)
func postScroll(_ event: CGEvent) {
    guard NSWorkspace.shared.frontmostApplication?.processIdentifier == app.processIdentifier else {
        fatalError("Dedicated app lost foreground; stopping rather than sending input to another app")
    }
    event.post(tap: .cgSessionEventTap)
}
let x = Double(option("x", "850"))!, y = Double(option("y", "400"))!
let point = CGPoint(x: bounds.minX + x, y: bounds.minY + y)
guard bounds.contains(point) else { fatalError("Scroll point outside verification window") }
app.activate(options: [.activateAllWindows])
AXUIElementPerformAction(window, kAXRaiseAction as CFString)
Thread.sleep(forTimeInterval: 0.3)
func captureFrame(_ label: String) {
    let metadata = URL(fileURLWithPath: capture.path + ".render.json")
    let oldDate = (try? FileManager.default.attributesOfItem(atPath: metadata.path)[.modificationDate]) as? Date
    for down in [true, false] {
        let key = CGEvent(keyboardEventSource: nil, virtualKey: 111, keyDown: down)!
        key.flags = [.maskCommand, .maskShift]
        key.postToPid(app.processIdentifier)
        Thread.sleep(forTimeInterval: 0.08)
    }
    var fresh = false
    for _ in 0..<200 {
        Thread.sleep(forTimeInterval: 0.05)
        let date = (try? FileManager.default.attributesOfItem(atPath: metadata.path)[.modificationDate]) as? Date
        if date != nil && date != oldDate { fresh = true; break }
    }
    precondition(fresh, "App did not produce a fresh renderer capture")
    for source in [capture, metadata] {
        let destination = output.appendingPathComponent(label + (source == capture ? ".png" : ".render.json"))
        try? FileManager.default.removeItem(at: destination)
        try! FileManager.default.copyItem(at: source, to: destination)
    }
}
let duration = Double(option("seconds", "16"))!
let halfPeriod = Double(option("half-period", "2"))!
let delta = Int32(option("delta", "8"))!
let scenario = option("scenario", "scroll")
guard duration > 0, halfPeriod > 0, ["scroll", "idle", "soak"].contains(scenario) else {
    fatalError("Use a positive duration/half-period and scroll, idle or soak scenario")
}
CGEvent(mouseEventSource: nil, mouseType: .mouseMoved, mouseCursorPosition: point, mouseButton: .left)!
    .post(tap: .cghidEventTap)
if CommandLine.arguments.contains("--reset-top") {
    let event = CGEvent(scrollWheelEvent2Source: nil, units: .pixel, wheelCount: 1,
                        wheel1: 1_000_000, wheel2: 0, wheel3: 0)!
    event.location = point
    event.timestamp = DispatchTime.now().uptimeNanoseconds
    postScroll(event)
    Thread.sleep(forTimeInterval: 1)
}
captureFrame("before")
let start = ProcessInfo.processInfo.systemUptime
let wallStart = Date().timeIntervalSince1970
let recordURL = output.appendingPathComponent("workload.jsonl")
FileManager.default.createFile(atPath: recordURL.path, contents: nil)
let log = try FileHandle(forWritingTo: recordURL)
func perfTime() -> Double? {
    guard !option("perf-log").isEmpty,
          let file = FileHandle(forReadingAtPath: option("perf-log")) else { return nil }
    defer { try? file.close() }
    let length = (try? file.seekToEnd()) ?? 0
    try? file.seek(toOffset: length > 8192 ? length - 8192 : 0)
    guard let data = try? file.readToEnd(), let text = String(data: data, encoding: .utf8),
          let last = text.split(separator: "\n").last,
          let row = try? JSONSerialization.jsonObject(with: Data(last.utf8)) as? [String: Any] else { return nil }
    return row["t_ms"] as? Double
}
func record(_ data: [String: Any]) {
    try! log.write(contentsOf: JSONSerialization.data(withJSONObject: data, options: [.sortedKeys]) + Data([10]))
}
record(["event": "start", "utc": wallStart, "pid": app.processIdentifier, "scenario": scenario,
        "seconds": duration, "width": bounds.width, "height": bounds.height,
        "point": [x,y], "delta": delta, "halfPeriod": halfPeriod, "perfMs": perfTime() as Any? ?? NSNull()])
var ticks = 0, nextTick = start, nextReport = start + 30
var lastSidebar: Bool?
while ProcessInfo.processInfo.systemUptime - start < duration {
    guard !app.isTerminated else { fatalError("Verification app exited during workload") }
    let now = ProcessInfo.processInfo.systemUptime
    let elapsed = now - start
    // Soak cycles: 20s conversation, 20s sidebar, 20s idle. Input is 60Hz.
    let cyclePhase = Int(elapsed) % 60
    let scrolling = scenario == "scroll" || (scenario == "soak" && cyclePhase < 40)
    if scrolling {
        let sidebar = scenario == "soak" && cyclePhase >= 20
        let location = sidebar ? CGPoint(x: bounds.minX + 120, y: bounds.minY + 500) : point
        if lastSidebar != sidebar {
            CGEvent(mouseEventSource: nil, mouseType: .mouseMoved, mouseCursorPosition: location, mouseButton: .left)!
                .post(tap: .cghidEventTap)
            lastSidebar = sidebar
        }
        let direction: Int32 = Int(elapsed / halfPeriod) % 2 == 0 ? -1 : 1
        let event = CGEvent(scrollWheelEvent2Source: nil, units: .pixel, wheelCount: 1,
                            wheel1: direction * delta, wheel2: 0, wheel3: 0)!
        event.location = location
        event.timestamp = DispatchTime.now().uptimeNanoseconds
        postScroll(event)
        ticks += 1
    }
    if now >= nextReport {
        record(["event": "progress", "elapsed": elapsed, "inputs": ticks,
                "perfMs": perfTime() as Any? ?? NSNull()])
        nextReport += 30
    }
    nextTick += 1.0 / 60
    let wait = nextTick - ProcessInfo.processInfo.systemUptime
    if wait > 0 { Thread.sleep(forTimeInterval: wait) } else { nextTick = ProcessInfo.processInfo.systemUptime }
}
Thread.sleep(forTimeInterval: 0.2) // Drain the final delivered input/presentation.
record(["event": "end", "elapsed": ProcessInfo.processInfo.systemUptime - start, "inputs": ticks,
        "utc": Date().timeIntervalSince1970, "perfMs": perfTime() as Any? ?? NSNull()])
try log.close()
captureFrame("after")
print("Completed \(scenario): \(ticks) inputs over \(duration)s; evidence: \(output.path)")
