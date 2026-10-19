// Drive only the named verification window. Captures come from the app itself.
import AppKit
import ApplicationServices
import Foundation

func option(_ name: String, _ fallback: String = "") -> String {
    CommandLine.arguments.first { $0.hasPrefix("--\(name)=") }?.dropFirst(name.count + 3).description ?? fallback
}
let accessibility = AXIsProcessTrusted()
let postEvents = CGPreflightPostEventAccess()
if CommandLine.arguments.contains("--check-permissions") {
    print("{\"accessibility\":\(accessibility),\"postEvents\":\(postEvents),\"screenRecordingRequired\":false,\"appleEventsRequired\":false}")
    exit(accessibility && postEvents ? 0 : 2)
}
guard accessibility && postEvents else { fatalError("Missing Accessibility/input access; no permission request was made") }
let name = option("app")
guard !name.isEmpty, let app = NSWorkspace.shared.runningApplications.first(where: { $0.localizedName == name }) else { fatalError("Named verification instance missing") }
let root = AXUIElementCreateApplication(app.processIdentifier)
func value(_ e: AXUIElement, _ key: String) -> CFTypeRef? { var r: CFTypeRef?; AXUIElementCopyAttributeValue(e,key as CFString,&r);return r }
guard let window = (value(root,kAXWindowsAttribute) as? [AXUIElement])?.first else { fatalError("No verification window") }
var origin = CGPoint.zero
var size = CGSize.zero
if let p = value(window,kAXPositionAttribute) { AXValueGetValue(p as! AXValue,.cgPoint,&origin) }
if let s = value(window,kAXSizeAttribute) { AXValueGetValue(s as! AXValue,.cgSize,&size) }
let start = Double(option("start-x", "1080"))!
let end = Double(option("end-x", "750"))!
let y = Double(option("y", "400"))!
let seconds = Double(option("seconds", "2"))!
let cycles = Int(option("cycles", "4"))!
let hz = Double(option("hz", "120"))!
if size.width <= max(start,end) || size.height <= y {
    let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly,.excludeDesktopElements],kCGNullWindowID) as? [[String:Any]] ?? []
    let frames = windows.filter { ($0[kCGWindowOwnerPID as String] as? Int) == Int(app.processIdentifier) && ($0[kCGWindowLayer as String] as? Int) == 0 }.compactMap { info -> CGRect? in
        guard let frame=info[kCGWindowBounds as String] as? [String:Any] else {return nil}
        return CGRect(dictionaryRepresentation:frame as CFDictionary)
    }
    if let frame=frames.max(by:{$0.width*$0.height < $1.width*$1.height}) {origin=frame.origin;size=frame.size}
}
precondition(size.width > max(start,end) && min(start,end) >= 0 && y > 0 && y < size.height)
precondition(seconds > 0 && seconds <= 10 && cycles > 0 && cycles <= 20 && hz > 0 && hz <= 500)
let output = URL(fileURLWithPath: option("output"))
try FileManager.default.createDirectory(at:output,withIntermediateDirectories:true)
app.activate(options:[.activateAllWindows])
Thread.sleep(forTimeInterval:0.3)
func post(_ type: CGEventType, _ x: Double) {
    let e = CGEvent(mouseEventSource:nil,mouseType:type,mouseCursorPosition:CGPoint(x:origin.x+x,y:origin.y+y),mouseButton:.left)!
    e.flags=[];e.post(tap:.cghidEventTap)
}
// Optional fixed stream origin: the caller prepares a draft in the focused
// verification composer, then this driver submits it one second before dragging.
var submittedEpoch: Double? = nil
if CommandLine.arguments.contains("--submit-return") {
    submittedEpoch = Date().timeIntervalSince1970
    for down in [true,false] {
        let event=CGEvent(keyboardEventSource:nil,virtualKey:36,keyDown:down)!
        event.flags=[];event.postToPid(app.processIdentifier)
    }
    Thread.sleep(forTimeInterval:1.0)
}
let copyPath = option("verify-copy")
let pasteboard = NSPasteboard.general
var savedClipboard: [NSPasteboardItem]? = nil
var expectedCopy: String? = nil
if !copyPath.isEmpty {
    expectedCopy = try String(contentsOfFile:copyPath,encoding:.utf8)
    savedClipboard = (pasteboard.pasteboardItems ?? []).map { item in
        let copy=NSPasteboardItem()
        for type in item.types { if let data=item.data(forType:type) { copy.setData(data,forType:type) } }
        return copy
    }
}
func verifyCopy() {
    guard let expectedCopy else {return}
    pasteboard.clearContents();pasteboard.setString("Echora resize copy probe",forType:.string)
    for down in [true,false] {
        let event=CGEvent(keyboardEventSource:nil,virtualKey:8,keyDown:down)!
        event.flags=[.maskCommand];event.postToPid(app.processIdentifier)
    }
    let deadline=Date().addingTimeInterval(4)
    while pasteboard.string(forType:.string) != expectedCopy && Date()<deadline {Thread.sleep(forTimeInterval:0.05)}
    guard pasteboard.string(forType:.string) == expectedCopy else {
        if let savedClipboard {pasteboard.clearContents();pasteboard.writeObjects(savedClipboard)}
        fatalError("Copied selection differs from the configured fixture")
    }
}
verifyCopy()
var report: [[String:Any]] = []
for cycle in 0..<cycles {
    let began=Date().timeIntervalSince1970
    let clock=ProcessInfo.processInfo.systemUptime
    post(.mouseMoved,start);post(.leftMouseDown,start)
    let steps=Int(seconds*hz)
    for i in 0...steps {
        let t=Double(i)/Double(steps)
        let fraction=t<0.5 ? t*2 : (1-t)*2
        post(.leftMouseDragged,start+(end-start)*fraction)
        let deadline=clock+Double(i+1)/hz
        let remaining=deadline-ProcessInfo.processInfo.systemUptime
        if remaining>0 {Thread.sleep(forTimeInterval:remaining)}
    }
    post(.leftMouseUp,start)
    let finished=Date().timeIntervalSince1970
    report.append(["cycle":cycle,"start_epoch":began,"end_epoch":finished,"posted_move_events":steps+1,"window_origin":[origin.x,origin.y],"window_size":[size.width,size.height],"start_x":start,"end_x":end,"y":y])
    if let submittedEpoch { report[report.count-1]["submit_epoch"] = submittedEpoch }
    Thread.sleep(forTimeInterval:0.3)
}
verifyCopy()
if let expectedCopy { report[report.count-1]["copied_utf8_bytes"] = expectedCopy.utf8.count }
if let savedClipboard {pasteboard.clearContents();pasteboard.writeObjects(savedClipboard)}
try JSONSerialization.data(withJSONObject:report,options:[.prettyPrinted,.sortedKeys]).write(to:output.appendingPathComponent("drag-input.json"))
// Leave the pointer over content, so hover cards do not contaminate capture.
post(.mouseMoved,end+20)
let capture=option("capture")
if !capture.isEmpty {
    precondition(capture.hasPrefix("/"))
    for suffix in ["", ".render.json"] {try? FileManager.default.removeItem(atPath:capture+suffix)}
    for down in [true,false] {let e=CGEvent(keyboardEventSource:nil,virtualKey:111,keyDown:down)!;e.flags=[.maskCommand,.maskShift];e.postToPid(app.processIdentifier)}
    let deadline=Date().addingTimeInterval(20)
    let source=URL(fileURLWithPath:capture)
    while (!FileManager.default.fileExists(atPath:source.path) || !FileManager.default.fileExists(atPath:source.path+".render.json")) && Date()<deadline {Thread.sleep(forTimeInterval:0.05)}
    for suffix in ["", ".render.json"] {
        let destination=output.appendingPathComponent("after.png"+suffix)
        try? FileManager.default.removeItem(at:destination)
        try FileManager.default.copyItem(at:URL(fileURLWithPath:capture+suffix),to:destination)
    }
}
print("Completed \(cycles) round-trip drags in named instance \(name), PID \(app.processIdentifier)")
