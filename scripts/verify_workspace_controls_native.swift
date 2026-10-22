// Drive only the named verification app using its accessibility tree.
import AppKit
import ApplicationServices
import Foundation

func option(_ name: String, _ fallback: String = "") -> String {
    CommandLine.arguments.first(where: { $0.hasPrefix("--\(name)=") })?.dropFirst(name.count + 3).description ?? fallback
}
let name = option("app")
let accessibility = AXIsProcessTrusted()
let postEvents = CGPreflightPostEventAccess()
if CommandLine.arguments.contains("--check-permissions") {
    let data = try JSONSerialization.data(withJSONObject: ["accessibility": accessibility, "postEvents": postEvents, "screenRecordingRequired": false, "appleEventsRequired": false], options: [.sortedKeys])
    print(String(data: data, encoding: .utf8)!)
    exit(accessibility && postEvents ? 0 : 2)
}
guard accessibility && postEvents else {
    fputs("Accessibility and input access are missing for the app running this driver. Grant them once in System Settings; this driver never requests a permission prompt.\n", stderr)
    exit(2)
}
let output = URL(fileURLWithPath: option("output", "artifacts/issues-23-24/native"))
let capturePath = option("capture")
guard capturePath.hasPrefix("/") else {
    fputs("Pass --capture=/absolute/frame.png matching the verification app's GPUI_CAPTURE_OUTPUT on the internal disk.\n", stderr)
    exit(2)
}
let capture = URL(fileURLWithPath: capturePath)
let inventory = NSWorkspace.shared.runningApplications.map { ["name": $0.localizedName ?? "", "pid": String($0.processIdentifier)] }
guard !name.isEmpty, let app = NSWorkspace.shared.runningApplications.first(where: { $0.localizedName == name }) else { fatalError("Named verification app missing") }
try FileManager.default.createDirectory(at: output, withIntermediateDirectories: true)
try JSONSerialization.data(withJSONObject: inventory, options: [.prettyPrinted]).write(to: output.appendingPathComponent("applications.json"))
let root = AXUIElementCreateApplication(app.processIdentifier)
func value(_ e: AXUIElement, _ key: String) -> CFTypeRef? { var result: CFTypeRef?; AXUIElementCopyAttributeValue(e, key as CFString, &result); return result }
func elements() -> [AXUIElement] {
    func descendants(_ e: AXUIElement) -> [AXUIElement] {
        (value(e, kAXChildrenAttribute) as? [AXUIElement] ?? []).flatMap { [$0] + descendants($0) }
    }
    return (value(root, kAXWindowsAttribute) as? [AXUIElement] ?? []).flatMap { [$0] + descendants($0) }
}
func title(_ e: AXUIElement) -> String { value(e, kAXTitleAttribute) as? String ?? value(e, kAXDescriptionAttribute) as? String ?? "" }
func find(_ label: String) -> AXUIElement {
    for _ in 0..<50 {
        if let e = elements().first(where: { title($0) == label || value($0, kAXIdentifierAttribute) as? String == label }) { return e }
        Thread.sleep(forTimeInterval: 0.05)
    }
    record("failure")
    fatalError("Missing accessibility control: \(label)")
}
func rect(_ e: AXUIElement) -> CGRect {
    var point = CGPoint.zero; var size = CGSize.zero
    if let v = value(e, kAXPositionAttribute) { AXValueGetValue(v as! AXValue, .cgPoint, &point) }
    if let v = value(e, kAXSizeAttribute) { AXValueGetValue(v as! AXValue, .cgSize, &size) }
    return CGRect(origin: point, size: size)
}
func key(_ code: Int, command: Bool = false, shift: Bool = false) {
    app.activate(options: [.activateAllWindows])
    Thread.sleep(forTimeInterval: 0.2)
    var flags: CGEventFlags = []
    if command { flags.insert(.maskCommand) }
    if shift { flags.insert(.maskShift) }
    for down in [true, false] {
        let event = CGEvent(keyboardEventSource: nil, virtualKey: CGKeyCode(code), keyDown: down)!
        event.flags = flags
        event.postToPid(app.processIdentifier)
        Thread.sleep(forTimeInterval: 0.08)
    }
    Thread.sleep(forTimeInterval: 0.2)
}
func text(_ text: String) {
    // Paste committed text so a composition-based input source cannot retain
    // the fixture text as an unfinished IME candidate.
    let pasteboard = NSPasteboard.general
    let saved = (pasteboard.pasteboardItems ?? []).map { item -> NSPasteboardItem in
        let copy = NSPasteboardItem()
        for type in item.types { if let data = item.data(forType: type) { copy.setData(data, forType: type) } }
        return copy
    }
    pasteboard.clearContents(); pasteboard.setString(text, forType: .string)
    key(9, command: true)
    // The native event queue can lag behind the driver during a new draft's
    // first layout. Keep the fixture available until the app consumes Paste.
    for _ in 0..<100 {
        if elements().contains(where: { (value($0, kAXValueAttribute) as? String)?.contains(text) == true }) { break }
        Thread.sleep(forTimeInterval: 0.05)
    }
    pasteboard.clearContents(); pasteboard.writeObjects(saved)
    Thread.sleep(forTimeInterval: 0.2)
}
func click(_ e: AXUIElement, edge: Int = 0) {
    app.activate(options: [.activateAllWindows])
    Thread.sleep(forTimeInterval: 0.1)
    let r = rect(e); let p = CGPoint(x: edge < 0 ? r.minX + 2 : edge > 0 ? r.maxX - 2 : r.midX, y: r.midY)
    for type in [CGEventType.mouseMoved, .leftMouseDown, .leftMouseUp] {
        let event = CGEvent(mouseEventSource: nil, mouseType: type, mouseCursorPosition: p, mouseButton: .left)!
        event.flags = []
        event.post(tap: .cghidEventTap)
        Thread.sleep(forTimeInterval: 0.06)
    }
    Thread.sleep(forTimeInterval: 0.3)
}
var report: [[String: Any]] = []
func record(_ label: String) {
    let target = output.appendingPathComponent(label + ".png")
    // The app captures its own renderer, which needs no Screen Recording grant.
    try? FileManager.default.removeItem(at: capture)
    let metadata = URL(fileURLWithPath: capture.path + ".render.json")
    try? FileManager.default.removeItem(at: metadata)
    key(111, command: true, shift: true)
    for _ in 0..<600 {
        if FileManager.default.fileExists(atPath: capture.path) && FileManager.default.fileExists(atPath: metadata.path) { break }
        Thread.sleep(forTimeInterval: 0.05)
    }
    let data = try! Data(contentsOf: capture)
    precondition(NSBitmapImageRep(data: data) != nil, "App renderer did not write a fresh PNG")
    try! data.write(to: target)
    try! Data(contentsOf: metadata).write(to: output.appendingPathComponent(label + ".png.render.json"))
    let controls = elements().map { e -> [String: Any] in
        let r = rect(e)
        return ["role": value(e, kAXRoleAttribute) as? String ?? "", "title":title(e), "id":value(e, kAXIdentifierAttribute) as? String ?? "", "value":value(e, kAXValueAttribute) as? String ?? "", "rect":[r.minX,r.minY,r.width,r.height]]
    }
    report.append(["name":label,"controls":controls])
    try! JSONSerialization.data(withJSONObject: report, options: [.prettyPrinted, .sortedKeys]).write(to: output.appendingPathComponent("report.json"))
}
func restoreFromDock() {
    let query = Process(), pipe = Pipe()
    query.executableURL = URL(fileURLWithPath: "/usr/bin/pgrep")
    query.arguments = ["-x", "Dock"]; query.standardOutput = pipe
    try! query.run(); query.waitUntilExit()
    let dockPid = Int32(String(data: pipe.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8)!.trimmingCharacters(in: .whitespacesAndNewlines))!
    let dockRoot = AXUIElementCreateApplication(dockPid)
    func descendants(_ e: AXUIElement) -> [AXUIElement] {
        (value(e, kAXChildrenAttribute) as? [AXUIElement] ?? []).flatMap { [$0] + descendants($0) }
    }
    guard let item = descendants(dockRoot).first(where: { title($0) == name }) else {
        fatalError("Missing Dock item for the dedicated app: \(name)")
    }
    // Dock items can sit outside the screen when auto-hide is enabled.
    precondition(AXUIElementPerformAction(item, kAXPressAction as CFString) == .success,
                 "The dedicated Dock icon must expose its Open action")
    RunLoop.current.run(until: Date().addingTimeInterval(0.4))
}
func mainWindow() -> AXUIElement {
    (value(root, kAXWindowsAttribute) as! [AXUIElement]).max { rect($0).width * rect($0).height < rect($1).width * rect($1).height }!
}
if option("scenario") == "notifications" {
    precondition(title(find("查看活动")) == "查看活动", "Read history must not light the bell")
    record("00-read-history")
    for (index, prompt) in ["PLAIN_STREAM", "FAIL_STREAM"].enumerated() {
        click(find("新聊天"))
        click(find("聊天输入框")); text(prompt); key(36)
        // Switch away as soon as the submission is accepted; the stop
        // control can appear after asynchronous thread creation.
        click(find("新聊天"))
        for _ in 0..<1000 {
            if elements().contains(where: { title($0) == "查看活动，需要关注" }) { break }
            RunLoop.current.run(until: Date().addingTimeInterval(0.05))
        }
        click(find("查看活动，需要关注"))
        record("0\(index * 3 + 1)-unread-activity")
        click(find("活动视图选项"))
        click(find("全部标为已读"))
        click(find("关闭活动视图"))
        _ = find("查看活动")
        precondition(!elements().contains(where: { title($0) == "查看活动，需要关注" }))
        record("0\(index * 3 + 2)-marked-read")
    }
    print("Verified successful and failed background completions light the bell and Mark all as read clears it")
    exit(0)
}
if option("scenario") == "quit-relaunch" {
    let oldPid = app.processIdentifier
    let window = mainWindow()
    // A synchronous process exit may invalidate the AX reply itself.
    _ = AXUIElementPerformAction(value(window, kAXCloseButtonAttribute) as! AXUIElement, kAXPressAction as CFString)
    for _ in 0..<100 {
        RunLoop.current.run(until: Date().addingTimeInterval(0.05))
        if app.isTerminated { break }
    }
    precondition(app.isTerminated, "Red close must end the process in quit mode")
    restoreFromDock()
    for _ in 0..<100 {
        if NSWorkspace.shared.runningApplications.contains(where: { $0.localizedName == name && $0.processIdentifier != oldPid }) { break }
        RunLoop.current.run(until: Date().addingTimeInterval(0.1))
    }
    guard let relaunched = NSWorkspace.shared.runningApplications.first(where: { $0.localizedName == name && $0.processIdentifier != oldPid }) else { fatalError("Dock must launch a new instance after a full quit") }
    try! JSONSerialization.data(withJSONObject: ["oldPid":oldPid,"newPid":relaunched.processIdentifier,"dockRelaunched":true], options: [.prettyPrinted]).write(to:output.appendingPathComponent("quit-relaunch.json"))
    print("Verified full quit and actual Dock relaunch with a new process")
    exit(0)
}
if option("scenario") == "reference-close" {
    let window = mainWindow()
    precondition(AXUIElementPerformAction(value(window, kAXCloseButtonAttribute) as! AXUIElement, kAXPressAction as CFString) == .success)
    RunLoop.current.run(until: Date().addingTimeInterval(0.4))
    precondition(!app.isTerminated, "Reference should retain its process")
    restoreFromDock()
    precondition(!app.isTerminated && !app.isHidden && !elements().isEmpty, "Reference Dock reopen failed")
    try! JSONSerialization.data(withJSONObject: ["pid":app.processIdentifier,"retainedProcess":true,"dockRestored":true], options: [.prettyPrinted]).write(to:output.appendingPathComponent("reference-lifecycle.json"))
    print("Verified the isolated reference red-close and actual Dock restore")
    exit(0)
}
if ["running-close", "running-close-started"].contains(option("scenario")) {
    if option("scenario") == "running-close" {
        click(find("聊天输入框")); text("STREAM"); key(36)
    }
    _ = find("停止")
    record("00-task-running")
    let window = mainWindow()
    precondition(AXUIElementPerformAction(value(window, kAXCloseButtonAttribute) as! AXUIElement, kAXPressAction as CFString) == .success)
    let hiddenAt = Date()
    RunLoop.current.run(until: hiddenAt.addingTimeInterval(35))
    precondition(app.isHidden && !app.isTerminated, "Task must stay alive while hidden")
    let restoredAt = Date()
    restoreFromDock()
    precondition(!app.isHidden && !app.isTerminated)
    precondition(!elements().contains { title($0) == "停止" }, "Fixture task must finish in the background")
    record("01-task-complete-after-dock")
    try! JSONSerialization.data(withJSONObject: ["pid":app.processIdentifier,"hiddenAt":hiddenAt.timeIntervalSince1970,"restoredAt":restoredAt.timeIntervalSince1970], options: [.prettyPrinted]).write(to:output.appendingPathComponent("hidden-interval.json"))
    print("Verified a running turn completes while the window is hidden and displays its result after Dock restore")
    exit(0)
}
if ["close-background", "close-preserve"].contains(option("scenario")) {
    if option("scenario") == "close-background" {
        let setting = find(option("close-label", "关闭窗口后在后台保留 Echora"))
        if (value(setting, kAXValueAttribute) as? NSNumber)?.boolValue != true { click(setting) }
    }
    record("00-background-enabled")
    let window = mainWindow()
    let initialRect = rect(window)
    for cycle in 1...3 {
        let close = value(window, kAXCloseButtonAttribute) as! AXUIElement
        // macOS can place its purple sharing indicator over the traffic
        // lights while Computer Use is connected. Press the native close
        // control itself so the indicator cannot intercept the click.
        precondition(AXUIElementPerformAction(close, kAXPressAction as CFString) == .success)
        RunLoop.current.run(until: Date().addingTimeInterval(0.4))
        precondition(!app.isTerminated && app.isHidden, "Closing must hide the live process")
        restoreFromDock()
        for _ in 0..<40 {
            if !app.isHidden { break }
            RunLoop.current.run(until: Date().addingTimeInterval(0.05))
        }
        precondition(!app.isHidden && !app.isTerminated, "Dock must restore the same process")
        let windows = value(root, kAXWindowsAttribute) as! [AXUIElement]
        precondition(windows.filter { title($0) == title(window) }.count == 1 && CFEqual(mainWindow(), window), "Dock must restore the original window once")
        precondition(rect(mainWindow()) == initialRect, "Window geometry must survive closing")
        let expected = option("expected-text")
        if !expected.isEmpty {
            precondition(elements().contains { (value($0, kAXValueAttribute) as? String)?.contains(expected) == true }, "Unsent draft must survive Dock reopen")
        }
        record("0\(cycle)-dock-restored")
    }
    print("Verified three red-close/Dock cycles preserve the original process, window, geometry and settings")
    exit(0)
}
if option("scenario") == "hover" {
    guard let x = Double(option("x")), let y = Double(option("y")),
          let window = (value(root, kAXWindowsAttribute) as? [AXUIElement])?.first else {
        fatalError("Hover capture requires --x and --y in window coordinates")
    }
    app.activate(options: [.activateAllWindows])
    var bounds = rect(window)
    // GPUI's first AX window can be a tooltip or have incomplete geometry.
    // Read window metadata (not screen pixels) for this process in that case.
    if bounds.width <= x || bounds.height <= y {
        let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] ?? []
        let frames = windows.filter {
            ($0[kCGWindowOwnerPID as String] as? Int) == Int(app.processIdentifier) &&
            ($0[kCGWindowLayer as String] as? Int) == 0
        }.compactMap { info -> CGRect? in
            guard let frame = info[kCGWindowBounds as String] as? [String: Any] else { return nil }
            return CGRect(dictionaryRepresentation: frame as CFDictionary)
        }
        if let resolved = frames.max(by: { $0.width * $0.height < $1.width * $1.height }) {
            bounds = resolved
        }
    }
    precondition(x.isFinite && y.isFinite && x >= 0 && y >= 0 && x < bounds.width && y < bounds.height,
                 "Hover coordinates must be inside the named verification window: \(bounds)")
    let point = CGPoint(x: bounds.minX + x, y: bounds.minY + y)
    CGEvent(mouseEventSource: nil, mouseType: .mouseMoved,
            mouseCursorPosition: point, mouseButton: .left)!.post(tap: .cghidEventTap)
    Thread.sleep(forTimeInterval: 0.8)
    record("00-hover")
    print("Captured pointer hover at \(x), \(y) in the named verification window")
    exit(0)
}
if option("scenario") == "capture" {
    record("00-capture")
    print("Verified app-owned capture without Screen Recording or Apple Events")
    exit(0)
}
if option("scenario") == "project-card-bottom" {
    key(53)
    let trigger=rect(find("Project actions for " + option("project")))
    let windowRect=rect(elements().first {value($0,kAXRoleAttribute) as? String == "AXWindow"}!)
    precondition(trigger.midY > windowRect.maxY-180, "Fixture must exercise the bottom window edge")
    let point=CGPoint(x:trigger.minX-70,y:trigger.midY)
    CGEvent(mouseEventSource:nil,mouseType:.mouseMoved,mouseCursorPosition:point,mouseButton:.left)!.post(tap:.cghidEventTap)
    Thread.sleep(forTimeInterval:1.2)
    let edit=find("Edit project")
    precondition(rect(edit).maxY <= windowRect.maxY-8, "Hover card edit action overflowed the window")
    record("00-bottom-card")
    click(edit);_ = find("Project name");record("01-bottom-edit");key(53)
    print("Verified bottom project card stays inside the viewport and Edit remains clickable")
    exit(0)
}
if option("scenario") == "project-actions" || option("scenario") == "project-actions-write" {
    var project = option("project", "Echora")
    func present(_ label: String) -> Bool { elements().contains { title($0) == label || value($0,kAXIdentifierAttribute) as? String == label } }
    func confirming() -> Bool {elements().contains {title($0).hasPrefix("Remove ") && title($0).hasSuffix("?") || title($0).hasPrefix("Archive ") && title($0).hasSuffix("?")}}
    func openActions(_ edge: Int = 0) {
        key(53)
        for _ in 0..<4 {
            click(find("Project actions for " + project),edge:edge)
            if present("Edit") {return}
        }
        fatalError("Project actions did not open")
    }
    func waitClosed() {
        for _ in 0..<150 {
            if !present("Edit project") && !confirming() {return}
            Thread.sleep(forTimeInterval:0.1)
        }
        record("failure-pending");fatalError("Project action did not complete")
    }
    key(53)
    let triggerRect=rect(find("Project actions for " + project))
    let hover=CGPoint(x:triggerRect.minX-70,y:triggerRect.midY)
    CGEvent(mouseEventSource:nil,mouseType:.mouseMoved,mouseCursorPosition:hover,mouseButton:.left)!.post(tap:.cghidEventTap)
    Thread.sleep(forTimeInterval:1.2)
    _ = find("Edit project");record("00-hover-card")
    click(find("Edit project"));_ = find("Project name");key(53)
    for edge in [-1,0,1] {openActions(edge);key(53)}
    openActions();record("01-menu")
    precondition(!present("Rename") && !present("Move up") && !present("New section…"))
    click(find(present("Unpin") ? "Unpin" : "Pin"));record("02-pinned")
    openActions();precondition(present("Unpin"));click(find(present("Unpin") ? "Unpin" : "Pin"));record("03-unpinned")
    openActions();click(find("Edit"));record("04-edit")
    for _ in 0..<15 {key(48)}
    for _ in 0..<15 {key(48,shift:true)}
    key(53);precondition(!present("Edit project"))
    openActions();click(find("Archive chats"));record("05-archive-confirm")
    click(find("Cancel"));precondition(!confirming())
    openActions();click(find("Remove project"));record("06-remove-confirm")
    key(53);precondition(!confirming())
    openActions();key(119);key(115);key(53);record("07-keyboard")
    openActions();click(find("Chat input"));record("08-outside")
    precondition(!present("Edit"))
    if option("scenario") == "project-actions-write" {
        precondition(project == "Issue 32 fixture", "Writes require the disposable acceptance project")
        click(find("Start new chat in " + project))
        openActions();click(find("Edit"));
        key(0,command:true);text("Issue 32 edited fixture")
        click(find("Change project marker"));click(find("Dollar"))
        let source=option("source")
        precondition(source.contains("issues-40-29-32/project-action-fixture"), "Pass the disposable fixture source")
        let unavailable=source+"-temporarily-unavailable"
        try FileManager.default.moveItem(atPath:source,toPath:unavailable)
        click(find("Save"));Thread.sleep(forTimeInterval:1)
        let retained=present("Edit project") && present("Save")
        try FileManager.default.moveItem(atPath:unavailable,toPath:source)
        precondition(retained, "Failed save must preserve its editor for retry")
        record("09-save-error")
        click(find("Save"));waitClosed();
        project="Issue 32 edited fixture"
        _ = find("Project actions for " + project);record("09-saved")
        openActions();click(find("Edit"));click(find("Remove folder"));record("10-empty-sources")
        click(find("Save"));waitClosed();record("11-cleared-sources")
        openActions();click(find("Reveal in Finder"));precondition(present("Edit"));record("11-disabled-reveal");key(53)
        openActions();click(find("Archive chats"));click(find("Archive all"));waitClosed();record("12-archived")
        openActions();click(find("Remove project"));click(find("Remove project"));waitClosed();record("13-removed")
        precondition(!present("Project actions for " + project))
        precondition(present("Choose project"), "Removed project remained selected in the draft")
    }
    print("Verified project actions hit areas, pin/unpin, editing, confirmations, keyboard and outside dismissal" + (option("scenario")=="project-actions-write" ? ", plus persisted edit, empty roots, archive and removal" : ""))
    exit(0)
}
if option("scenario") == "projects" {
    func openOptions(_ label: String, edge: Int = 0) {
        key(53)
        for _ in 0..<4 {
            click(find(label), edge: edge)
            if elements().contains(where: {title($0)=="Organize sidebar"}) {return}
            Thread.sleep(forTimeInterval:0.3)
        }
        fatalError("Sidebar menu did not open: \(label)")
    }
    let projects = "Project sidebar options"
    for edge in [-1, 0, 1] {
        openOptions(projects, edge: edge); key(53)
    }
    openOptions(projects); record("01-menu")
    precondition(!elements().contains(where: {title($0)=="Show"}), "Projects menu included Recents-only visibility options")
    click(find("Organize sidebar")); record("02-organize")
    click(find("In one list")); record("03-combined")
    precondition(!elements().contains(where: {title($0)==projects}), "Combined layout did not remove Projects")
    openOptions("Chat sidebar options"); click(find("Organize sidebar")); click(find("By project"))
    openOptions(projects); click(find("Sort chats by")); record("04-sort")
    click(find("Manual order")); record("05-manual")
    openOptions("Chat sidebar options"); click(find("Sort chats by")); record("06-shared-sort")
    click(find("Last updated"))
    openOptions(projects); key(125); key(124); key(123); key(53); record("07-keyboard-dismiss")
    precondition(!elements().contains(where: {title($0)=="Organize sidebar"}))
    openOptions(projects); click(find("Chat input")); record("08-outside-dismiss")
    precondition(!elements().contains(where: {title($0)=="Organize sidebar"}))
    print("Verified Projects menu hit areas, organization, shared sorting, keyboard navigation, and outside dismissal")
    exit(0)
}
if option("scenario") == "recents" {
    app.activate(options: [.activateAllWindows]); Thread.sleep(forTimeInterval: 0.5)
    func menuItem(_ label: String) -> AXUIElement {
        for _ in 0..<50 {
            if let e=elements().first(where: { title($0)==label && value($0,kAXRoleAttribute) as? String == "AXMenuItem" }) {return e}
            Thread.sleep(forTimeInterval: 0.05)
        }
        fatalError("Missing menu item: \(label)")
    }
    let options = "Chat sidebar options"
    func openRecents() {
        key(53)
        for _ in 0..<4 {
            click(find(options))
            if elements().contains(where: {title($0)=="Organize sidebar"}) {return}
            Thread.sleep(forTimeInterval:0.3)
        }
        fatalError("Recents menu did not open")
    }
    openRecents(); record("01-menu")
    click(find("Organize sidebar")); record("02-organize")
    click(find("In one list")); record("03-combined")
    openRecents(); click(find("Sort chats by")); record("04-sort")
    click(find("Manual order")); record("05-manual")
    openRecents(); key(125); key(124); key(123); key(53); record("06-keyboard-dismiss")
    precondition(!elements().contains(where: { title($0) == "Organize sidebar" }), "Escape did not dismiss menu")
    openRecents(); click(find("Organize sidebar")); click(find("By project"))
    openRecents(); click(menuItem("Projects")); record("07-projects-hidden")
    openRecents(); click(menuItem("Projects")); record("08-projects-restored")
    let newChat = elements().first(where: { title($0) == "New chat" && value($0,kAXRoleAttribute) as? String == "AXButton" })!
    click(newChat, edge: -1)
    text("Recents focus check")
    precondition(value(find("Chat input"), kAXValueAttribute) as? String == "Recents focus check", "New chat did not focus the composer")
    record("09-new-chat")
    key(0,command:true); key(51)
    print("Verified Recents menus, layout choices, sorting, keyboard dismissal, visibility, and New chat")
    exit(0)
}
func waitForBranches() {
    for _ in 0..<300 {
        let controls = elements()
        if !controls.contains(where: { title($0) == "Loading branches…" }) && controls.contains(where: { value($0, kAXRoleAttribute) as? String == "AXMenuItem" && title($0) != "Create and checkout new branch…" }) { return }
        Thread.sleep(forTimeInterval: 0.1)
    }
    record("failure")
    fatalError("Branch list did not finish loading")
}
func scrollBranches() {
    let menuRect = rect(find("Branches")); let p = CGPoint(x: menuRect.midX, y: menuRect.midY)
    CGEvent(mouseEventSource: nil, mouseType: .mouseMoved, mouseCursorPosition: p, mouseButton: .left)!.post(tap: .cghidEventTap)
    Thread.sleep(forTimeInterval: 0.2)
    for _ in 0..<45 {
        let event = CGEvent(scrollWheelEvent2Source: CGEventSource(stateID: .hidSystemState), units: .line, wheelCount: 1, wheel1: -10, wheel2: 0, wheel3: 0)!
        event.location = p
        // Quartz creates wheel events with timestamp zero; AppKit drops those.
        event.timestamp = DispatchTime.now().uptimeNanoseconds
        event.post(tap: .cgSessionEventTap)
        Thread.sleep(forTimeInterval: 0.03)
    }
    Thread.sleep(forTimeInterval: 0.3)
    record("09-scrolled")
    if !option("repository").isEmpty {
        precondition(menuRect.intersects(rect(find("topic/long-acceptance-branch-149"))), "Last branch was not reachable by scrolling")
    }
}
if option("scenario") == "code-review-capture" {
    let itemTitle = option("pr-title", "Publish native macOS, Windows and Linux installers")
    for _ in 0..<600 {
        if elements().contains(where: { title($0) == itemTitle }) { break }
        Thread.sleep(forTimeInterval: 0.1)
    }
    click(find(itemTitle))
    for _ in 0..<600 {
        if elements().contains(where: { title($0) == "Details" }) { break }
        Thread.sleep(forTimeInterval: 0.1)
    }
    _ = find("Details")
    if let all = elements().first(where: { title($0).hasPrefix("All, ") }) { click(all) }
    key(53)
    let window = rect(mainWindow())
    let point = CGPoint(x: window.midX, y: window.minY + 35)
    CGEvent(mouseEventSource: nil, mouseType: .mouseMoved, mouseCursorPosition: point, mouseButton: .left)!.post(tap: .cghidEventTap)
    Thread.sleep(forTimeInterval: 0.3)
    record("after-pr50")
    print("Captured code review with no active hover")
    exit(0)
}
if option("scenario") == "code-review-send-to-agent" {
    click(find("Send to agent"))
    let input = find("聊天输入框")
    let draft = value(input, kAXValueAttribute) as? String ?? ""
    precondition(draft.contains("pull request #50") && draft.contains("https://github.com/rita152/Echora/pull/50"), "Send to agent lost the selected item")
    record("send-to-agent-draft")
    click(input); key(0, command: true); key(51)
    precondition((value(find("聊天输入框"), kAXValueAttribute) as? String ?? "").isEmpty, "Verification draft was not cleared")
    click(find("Pull requests"))
    print("Verified Send to agent prefill and cleared the unsent draft")
    exit(0)
}
if ["code-review-pin", "code-review-pin-restored"].contains(option("scenario")) {
    let restored = option("scenario") == "code-review-pin-restored"
    let label = restored ? "Unpin pull request #50" : "Pin pull request #50"
    for _ in 0..<600 {
        if elements().filter({ title($0) == label }).count == 2 { break }
        Thread.sleep(forTimeInterval: 0.1)
    }
    let pins = elements().filter { title($0) == label }
    precondition(pins.count == 2, "Header and list did not share the persisted pin state")
    record(restored ? "pin-restored-after-restart" : "pin-before-restart")
    click(pins.last!)
    _ = find(restored ? "Pin pull request #50" : "Unpin pull request #50")
    record(restored ? "pin-cleaned-up" : "pin-persisted")
    print("Verified code review pin persistence checkpoint")
    exit(0)
}
if option("scenario") == "code-review" {
    app.activate(options: [.activateAllWindows])
    let prTitle=option("pr-title", "Publish native macOS, Windows and Linux installers")
    func waitUntil(_ reason:String,_ test:()->Bool) {
        for _ in 0..<600 { if test() { return };Thread.sleep(forTimeInterval:0.1) }
        record("failure");fatalError(reason)
    }
    func has(_ label:String)->Bool { elements().contains {title($0)==label} }
    func waitLoaded() {
        // Search is debounced; observe its loading state after the timer fires.
        Thread.sleep(forTimeInterval:0.6)
        waitUntil("Inbox did not load") {!has("Loading code review")}
        Thread.sleep(forTimeInterval:0.2)
    }
    func chooseKind(_ prefix:String) {click(elements().first {title($0).hasPrefix(prefix+", ")}!)}
    func filter(_ label:String) {click(find("Filter"));click(find(label));waitLoaded()}
    func resize(_ width:CGFloat,_ height:CGFloat) {
        let window=mainWindow()
        var size=CGSize(width:width,height:height)
        precondition(AXUIElementSetAttributeValue(window,kAXSizeAttribute as CFString,AXValueCreate(.cgSize,&size)!) == .success)
        Thread.sleep(forTimeInterval:0.5)
    }
    func hover(_ e:AXUIElement) {
        let r=rect(e);let event=CGEvent(mouseEventSource:nil,mouseType:.mouseMoved,mouseCursorPosition:CGPoint(x:r.midX,y:r.midY),mouseButton:.left)!
        event.post(tap:.cghidEventTap);Thread.sleep(forTimeInterval:0.7)
    }
    waitLoaded()
    click(find("Search pull requests and issues"));key(0,command:true);text("repo:rita152/Echora");waitLoaded()
    waitUntil("Current repository inbox did not load") {has(prTitle)}
    click(find(prTitle));waitUntil("PR detail not loaded") {has("Details")}
    record("00-pr-summary")
    let original=rect(find(prTitle));precondition(abs(original.height-57.25)<2,"List row height differs from reference")
    let pins=elements().filter {title($0)=="Pin pull request #50"}
    precondition(pins.count==2,"Expected list and header pin")
    hover(pins.last!);record("01-pin-hover")
    click(pins.last!,edge:-1);_ = find("Unpin pull request #50");record("02-pinned")
    click(elements().last {title($0)=="Unpin pull request #50"}!,edge:1);_ = find("Pin pull request #50")
    let clipboard=NSPasteboard.general
    let saved=(clipboard.pasteboardItems ?? []).map { item -> NSPasteboardItem in let copy=NSPasteboardItem();for type in item.types {if let data=item.data(forType:type){copy.setData(data,forType:type)}};return copy }
    click(find("Copy link"),edge:-1)
    precondition(clipboard.string(forType:.string)=="https://github.com/rita152/Echora/pull/50","Copy link used the wrong item")
    clipboard.clearContents();clipboard.writeObjects(saved)
    record("03-link-copied")
    click(find("More actions"),edge:1);_ = find("Fix findings");record("04-more-actions");key(53)
    click(find("Sort: Newest"),edge:-1);click(find("Recently updated"));waitLoaded();record("05-recently-updated")
    click(find("Sort: Recently updated"),edge:1);click(find("Newest"));waitLoaded()
    click(find("Search pull requests and issues"));text("no-such-echora-code-review-item-81");waitLoaded()
    waitUntil("Search did not reach empty state") {has("Clear filters") && !has(prTitle)}
    record("06-empty-search");click(find("Clear search"));waitLoaded()
    waitUntil("Search did not restore the PR list") {has(prTitle)}
    click(find("Changes"));waitUntil("Diff did not load") {has("Review options") || has("All changes") || has("Show file tree")};record("07-changes")
    click(find("Timeline"));record("08-timeline")
    click(find("Summary"));record("09-summary-restored")
    let split=find("Resize workspace panes");let sr=rect(split)
    let from=CGPoint(x:sr.midX,y:sr.midY),to=CGPoint(x:sr.midX+70,y:sr.midY)
    for (type,p) in [(CGEventType.mouseMoved,from),(.leftMouseDown,from),(.leftMouseDragged,to),(.leftMouseUp,to)] {
        CGEvent(mouseEventSource:nil,mouseType:type,mouseCursorPosition:p,mouseButton:.left)!.post(tap:.cghidEventTap);Thread.sleep(forTimeInterval:0.15)
    }
    precondition(abs(rect(find("Resize workspace panes")).midX-sr.midX)>40,"Separator drag did not resize the list")
    record("10-separator-dragged")
    resize(1100,800);record("11-narrow-window");resize(1582,1036)
    filter("Open");chooseKind("Issues");waitLoaded()
    let issue=elements().first {title($0).hasPrefix("Issue / PR 页面：") }!
    click(issue);waitUntil("Issue detail not loaded") {has("Details")}
    precondition(!has("Changes") && !has("Merge") && !has("Review pull request changes"),"Issue exposed a PR-only action")
    record("12-issue-summary")
    click(find("Timeline"));record("13-issue-timeline")
    click(find("Search pull requests and issues"));key(0,command:true);text("https://github.com/rita152/Echora/pull/50?diff=split#issuecomment-1")
    waitUntil("Pasted PR link did not bypass issue/open filters") {has("Changes") && has("Details")}
    record("14-pasted-pr-link")
    click(find("More code review actions"));click(find("Refresh"));waitLoaded();record("15-refresh")
    click(find("Search pull requests and issues"));key(0,command:true);text("repo:rita152/Echora");waitLoaded()
    filter("Merged");chooseKind("All");waitLoaded()
    waitUntil("Merged repository list did not restore") {has("Show more")}
    click(find("Show more"));record("16-show-more");_ = find("Show less")
    func wheel(_ point:CGPoint,_ delta:Int32) {
        let event=CGEvent(scrollWheelEvent2Source:CGEventSource(stateID:.hidSystemState),units:.pixel,wheelCount:1,wheel1:delta,wheel2:0,wheel3:0)!
        event.location=point;event.timestamp=DispatchTime.now().uptimeNanoseconds;event.post(tap:.cgSessionEventTap);Thread.sleep(forTimeInterval:0.4)
    }
    let listPoint=CGPoint(x:original.midX,y:original.midY+250)
    wheel(listPoint,-700);record("17-list-scrolled");wheel(listPoint,1400)
    let separator=rect(find("Resize workspace panes"))
    wheel(CGPoint(x:separator.midX+150,y:separator.midY),-700);record("18-detail-scrolled");wheel(CGPoint(x:separator.midX+150,y:separator.midY),2000)
    wheel(listPoint,-900)
    click(find("Show less"));record("19-show-less")
    print("Verified code review native controls; \(report.count) captures in \(output.path)")
    exit(0)
}
if option("scenario") == "scroll" {
    scrollBranches()
    exit(0)
}
app.activate(options: [.activateAllWindows]); Thread.sleep(forTimeInterval: 0.5)
key(53)
click(find("Start new chat in " + option("project", "Echora")))
Thread.sleep(forTimeInterval: 0.8)
_ = find("composer-branch")
record("00-initial")
click(find("composer-location"), edge: -1)
_ = find("New local worktree")
record("01-location-icon-edge")
key(53)
Thread.sleep(forTimeInterval: 0.3)
click(find("composer-location"), edge: 1)
_ = find("New local worktree")
record("02-location-padding-edge")
click(find("New local worktree"))
precondition(rect(find("composer-location")).width > 120, "Worktree mode was not selected")
record("03-worktree-selected")
click(find("composer-branch"))
waitForBranches()
record("04-worktree-base")
key(53)
click(find("composer-location")); click(find("Local"))
record("05-local-restored")
click(find("composer-branch"), edge: -1)
_ = find("Search branches")
waitForBranches()
record("06-branches")
text("no-such-native-acceptance-branch")
record("07-empty-search")
key(0, command: true); key(51)
precondition(value(find("Search branches"), kAXValueAttribute) as? String == "", "Search was not cleared")
record("08-search-cleared")
scrollBranches()
click(find("Create and checkout new branch…"))
_ = find("Branch name")
record("10-create-branch")
key(53)
precondition(!elements().contains { title($0) == "Branch name" }, "Escape did not close the dialog")
record("11-create-cancelled")
click(find("composer-branch"), edge: 1)
record("12-branch-padding-edge")
key(53)
click(find("composer-branch")); waitForBranches()
click(find("Create and checkout new branch…")); _ = find("Branch name")
click(find("Close dialog"))
precondition(!elements().contains { title($0) == "Branch name" }, "Close button did not dismiss the dialog")
click(find("composer-branch")); waitForBranches()
click(find("Create and checkout new branch…")); click(find("Set prefix"))
_ = find("Branch prefix")
record("12a-prefix-settings")
click(find("Back to app")); _ = find("composer-branch")
if !option("repository").isEmpty {
    let repository = option("repository")
    func git(_ args: [String]) -> String {
        let process = Process(); let pipe = Pipe()
        process.executableURL = URL(fileURLWithPath: "/Library/Developer/CommandLineTools/usr/bin/git")
        process.arguments = ["-C", repository] + args; process.standardOutput = pipe
        try! process.run(); let data = pipe.fileHandleForReading.readDataToEndOfFile(); process.waitUntilExit()
        precondition(process.terminationStatus == 0)
        return String(data: data, encoding: .utf8)!.trimmingCharacters(in: .whitespacesAndNewlines)
    }
    func waitFor(_ message: String, _ condition: () -> Bool) {
        for _ in 0..<300 { if condition() { return }; Thread.sleep(forTimeInterval: 0.1) }
        fatalError(message)
    }
    func switchTo(_ branch: String, pointer: Bool = false) {
        click(find("composer-branch")); _ = find("Search branches"); text(branch)
        precondition(value(find("Search branches"), kAXValueAttribute) as? String == branch, "Branch search text differs")
        let row = find(branch)
        if pointer { click(row) } else { key(125); key(36) }
        waitFor("Git did not switch to " + branch) { git(["branch", "--show-current"]) == branch }
    }
    switchTo("topic/long-acceptance-branch-149", pointer: true)
    record("13-real-branch-switch")
    click(find("composer-branch")); _ = find("Search branches")
    click(find("Create and checkout new branch…"))
    let branch = "codex/native-acceptance-" + String(Int(Date().timeIntervalSince1970))
    _ = find("Branch name"); key(0, command: true); text(branch); key(36)
    waitFor("New branch was not checked out") { git(["branch", "--show-current"]) == branch }
    record("14-real-branch-created")
    switchTo("main")
    _ = git(["branch", "-d", branch])
    let before = git(["worktree", "list", "--porcelain"])
    let previousPaths = Set(before.components(separatedBy: "\n").filter { $0.hasPrefix("worktree ") }.map { String($0.dropFirst(9)) })
    click(find("composer-location")); click(find("New local worktree"))
    key(53)
    precondition(before == git(["worktree", "list", "--porcelain"]), "Mode selection allocated a checkout")
    click(find("Chat input")); text("!pwd > native-acceptance-cwd.txt"); key(36)
    var target = ""
    waitFor("First send did not use a new worktree") {
        for line in git(["worktree", "list", "--porcelain"]).components(separatedBy: "\n") where line.hasPrefix("worktree ") {
            let path = String(line.dropFirst(9))
            guard !previousPaths.contains(path), let contents = try? String(contentsOfFile: path + "/native-acceptance-cwd.txt", encoding: .utf8) else { continue }
            if contents.trimmingCharacters(in: .whitespacesAndNewlines) == path { target = path; return true }
        }
        return false
    }
    precondition(git(["branch", "--show-current"]) == "main")
    record("15-first-send-worktree")
    try! JSONSerialization.data(withJSONObject: ["source":repository, "worktree":target, "sourceBranch":"main", "passed":true], options: [.prettyPrinted]).write(to: output.appendingPathComponent("operations.json"))
}
print("Verified native workspace menus; \(report.count) captures in \(output.path)")
