// Titlebar zoom/restore in one explicitly named verification instance.
import AppKit
import ApplicationServices
import Foundation
import QuartzCore

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
func value(_ element: AXUIElement, _ key: String) -> CFTypeRef? {
    var result: CFTypeRef?
    AXUIElementCopyAttributeValue(element,key as CFString,&result)
    return result
}
guard let window = (value(root,kAXWindowsAttribute) as? [AXUIElement])?.first else { fatalError("No verification window") }
func frame() -> CGRect {
    // GPUI's cached AX window size can be stale during an AppKit animation.
    // Window-server geometry is metadata; it requires no screen recording.
    let windows=CGWindowListCopyWindowInfo([.optionOnScreenOnly,.excludeDesktopElements],kCGNullWindowID) as? [[String:Any]] ?? []
    let frames=windows.filter { ($0[kCGWindowOwnerPID as String] as? Int)==Int(app.processIdentifier) && ($0[kCGWindowLayer as String] as? Int)==0 }.compactMap { info -> CGRect? in
        guard let bounds=info[kCGWindowBounds as String] as? [String:Any] else {return nil}
        return CGRect(dictionaryRepresentation:bounds as CFDictionary)
    }
    if let result=frames.max(by:{$0.width*$0.height < $1.width*$1.height}) {return result}
    var origin = CGPoint.zero
    var size = CGSize.zero
    guard let p = value(window,kAXPositionAttribute), let s = value(window,kAXSizeAttribute),
          AXValueGetValue(p as! AXValue,.cgPoint,&origin), AXValueGetValue(s as! AXValue,.cgSize,&size) else { fatalError("Cannot read verification window frame") }
    return CGRect(origin:origin,size:size)
}
func jsonFrame(_ rect: CGRect) -> [String:Double] {
    ["x":rect.minX,"y":rect.minY,"width":rect.width,"height":rect.height]
}
let output = URL(fileURLWithPath: option("output"))
try FileManager.default.createDirectory(at:output,withIntermediateDirectories:true)
let cycles = Int(option("cycles","2"))!
let seconds = Double(option("seconds","2"))!
let titleX = Double(option("x","600"))!
let titleY = Double(option("y","20"))!
precondition(cycles > 0 && cycles <= 10 && seconds >= 1 && seconds <= 10)
app.activate(options:[.activateAllWindows])
Thread.sleep(forTimeInterval:0.3)
var submitted: Double? = nil
if CommandLine.arguments.contains("--submit-return") {
    submitted = CACurrentMediaTime()
    for down in [true,false] {
        let event=CGEvent(keyboardEventSource:nil,virtualKey:36,keyDown:down)!
        event.flags=[];event.postToPid(app.processIdentifier)
    }
    Thread.sleep(forTimeInterval:1)
}
var rows: [[String:Any]] = []
let initial=frame()
for index in 0..<(cycles*2) {
    let before=frame()
    precondition(titleX > 80 && titleX < before.width && titleY > 0 && titleY < 46)
    let position=CGPoint(x:before.minX+titleX,y:before.minY+titleY)
    let started=CACurrentMediaTime()
    var actionTime=started
    for count in [1,2] {
        for type in [CGEventType.leftMouseDown,CGEventType.leftMouseUp] {
            let event=CGEvent(mouseEventSource:nil,mouseType:type,mouseCursorPosition:position,mouseButton:.left)!
            event.setIntegerValueField(.mouseEventClickState,value:Int64(count))
            if count == 2 && type == .leftMouseUp { actionTime=CACurrentMediaTime() }
            event.flags=[];event.post(tap:.cghidEventTap)
            Thread.sleep(forTimeInterval:0.025)
        }
    }
    var samples: [[String:Any]]=[]
    while CACurrentMediaTime()-started < seconds {
        samples.append(["host_time":CACurrentMediaTime(),"frame":jsonFrame(frame())])
        Thread.sleep(forTimeInterval:0.05)
    }
    let after=frame()
    guard abs(before.width-after.width)>1 || abs(before.height-after.height)>1 else {fatalError("Double click did not change the verification window size")}
    if index%2 == 1 {
        guard abs(initial.width-after.width)<=1 && abs(initial.height-after.height)<=1 && abs(initial.minX-after.minX)<=1 && abs(initial.minY-after.minY)<=1 else {fatalError("Restore did not recover the original frame")}
    }
    var row:[String:Any]=["index":index,"direction":index%2==0 ? "zoom" : "restore","sequence_start_host_time":started,"input_host_time":actionTime,"end_host_time":CACurrentMediaTime(),"before":jsonFrame(before),"after":jsonFrame(after),"samples":samples]
    if let submitted {row["submit_host_time"]=submitted}
    rows.append(row)
}
let capture=option("capture")
if !capture.isEmpty {
    precondition(capture.hasPrefix("/"))
    for suffix in ["", ".render.json"] {try? FileManager.default.removeItem(atPath:capture+suffix)}
    for down in [true,false] {
        let event=CGEvent(keyboardEventSource:nil,virtualKey:111,keyDown:down)!
        event.flags=[.maskCommand,.maskShift];event.postToPid(app.processIdentifier)
    }
    let deadline=Date().addingTimeInterval(20)
    while (!FileManager.default.fileExists(atPath:capture) || !FileManager.default.fileExists(atPath:capture+".render.json")) && Date()<deadline {Thread.sleep(forTimeInterval:0.05)}
    for suffix in ["", ".render.json"] {
        let destination=output.appendingPathComponent("restored.png"+suffix)
        try? FileManager.default.removeItem(at:destination)
        try FileManager.default.copyItem(at:URL(fileURLWithPath:capture+suffix),to:destination)
    }
}
try JSONSerialization.data(withJSONObject:rows,options:[.prettyPrinted,.sortedKeys]).write(to:output.appendingPathComponent("zoom-input.json"))
print("Completed \(cycles) zoom/restore pairs in named instance \(name), PID \(app.processIdentifier)")
