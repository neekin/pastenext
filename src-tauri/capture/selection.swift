// Native screenshot selection: one overlay for region, window and display modes.
import AppKit
import CoreGraphics

let selectionShade: CGFloat = 0.60

enum CaptureMode: String, CaseIterable { case region, window, screen }
struct CaptureTarget { let id: CGWindowID; let frame: NSRect; let title: String }
func quartzRect(_ rect: NSRect, primaryTop: CGFloat) -> CGRect {
    CGRect(x: rect.minX, y: primaryTop - rect.maxY, width: rect.width, height: rect.height)
}
func cocoaRect(_ rect: CGRect, primaryTop: CGFloat) -> NSRect {
    NSRect(x: rect.minX, y: primaryTop - rect.maxY, width: rect.width, height: rect.height)
}
func boundedRect(_ rect: NSRect, in bounds: NSRect) -> NSRect {
    NSRect(x: max(bounds.minX, min(bounds.maxX - rect.width, rect.minX)),
           y: max(bounds.minY, min(bounds.maxY - rect.height, rect.minY)),
           width: min(rect.width, bounds.width), height: min(rect.height, bounds.height))
}
func regionBetween(_ a: NSPoint, _ b: NSPoint, in bounds: NSRect) -> NSRect {
    NSRect(x: min(a.x,b.x), y: min(a.y,b.y), width: abs(a.x-b.x), height: abs(a.y-b.y)).intersection(bounds)
}

final class SelectionWindow: NSWindow {
    weak var selector: ScreenshotSelector?
    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { true }
    override func keyDown(with event: NSEvent) { selector?.keyDown(event) }
}

final class SelectionView: NSView {
    weak var selector: ScreenshotSelector?
    let screen: NSScreen
    var pointer = NSPoint.zero
    private var start = NSPoint.zero
    private var original = NSRect.zero
    private var drag = ""
    init(screen: NSScreen, selector: ScreenshotSelector) {
        self.screen = screen; self.selector = selector
        super.init(frame: NSRect(origin:.zero,size:screen.frame.size))
    }
    required init?(coder: NSCoder) { fatalError("init(coder:) not supported") }
    override var isOpaque: Bool { false }
    override var acceptsFirstResponder: Bool { true }
    override func acceptsFirstMouse(for event:NSEvent?)->Bool {true}
    override func updateTrackingAreas() {
        trackingAreas.forEach(removeTrackingArea)
        addTrackingArea(NSTrackingArea(rect:bounds,options:[.activeAlways,.mouseMoved,.mouseEnteredAndExited,.inVisibleRect],owner:self,userInfo:nil))
        super.updateTrackingAreas()
    }
    override func resetCursorRects() { addCursorRect(bounds,cursor:.crosshair) }
    var localSelection: NSRect? {
        guard let rect=selector?.selectedRect else{return nil}
        let local=rect.offsetBy(dx:-screen.frame.minX,dy:-screen.frame.minY).intersection(bounds)
        return local.isEmpty ? nil : local
    }
    override func draw(_ dirtyRect: NSRect) {
        // Clear first: a transparent window must restore the old hole as it moves.
        NSColor.clear.setFill();bounds.fill(using:.copy)
        let mask=NSBezierPath(rect:bounds)
        if let selected=localSelection {mask.appendRect(selected)}
        mask.windingRule = .evenOdd
        NSColor.black.withAlphaComponent(selectionShade).setFill();mask.fill()
        guard let selected=localSelection else{return}
        NSColor.white.withAlphaComponent(0.95).setStroke()
        let border=NSBezierPath(rect:selected.insetBy(dx:0.5,dy:0.5));border.lineWidth=1.5;border.stroke()
        NSColor(calibratedRed:0.56,green:0.57,blue:1,alpha:1).setStroke()
        let outline=NSBezierPath(rect:selected.insetBy(dx:-1.5,dy:-1.5));outline.lineWidth=2;outline.stroke()
        if selector?.mode == .region {
            for point in handles(selected) {
                let handle=NSRect(x:point.x-3,y:point.y-3,width:6,height:6)
                NSColor.white.setFill();NSBezierPath(roundedRect:handle,xRadius:1,yRadius:1).fill()
            }
        }
        let scale=screen.backingScaleFactor
        let text="\(Int(selected.width*scale)) × \(Int(selected.height*scale)) px"
        let attributes:[NSAttributedString.Key:Any]=[.font:NSFont.monospacedDigitSystemFont(ofSize:12,weight:.medium),.foregroundColor:NSColor.white]
        let size=(text as NSString).size(withAttributes:attributes)
        let label=NSRect(x:min(max(8,selected.minX),bounds.width-size.width-24),y:min(bounds.height-30,max(8,selected.maxY+8)),width:size.width+16,height:24)
        NSColor.black.withAlphaComponent(0.85).setFill();NSBezierPath(roundedRect:label,xRadius:6,yRadius:6).fill()
        (text as NSString).draw(at:NSPoint(x:label.minX+8,y:label.minY+5),withAttributes:attributes)
    }
    func handles(_ rect:NSRect)->[NSPoint] {
        [NSPoint(x:rect.minX,y:rect.minY),NSPoint(x:rect.midX,y:rect.minY),NSPoint(x:rect.maxX,y:rect.minY),
         NSPoint(x:rect.minX,y:rect.midY),NSPoint(x:rect.maxX,y:rect.midY),
         NSPoint(x:rect.minX,y:rect.maxY),NSPoint(x:rect.midX,y:rect.maxY),NSPoint(x:rect.maxX,y:rect.maxY)]
    }
    func eventPoint(_ event:NSEvent)->NSPoint {
        let p=convert(event.locationInWindow,from:nil)
        return NSPoint(x:max(0,min(bounds.width,p.x)),y:max(0,min(bounds.height,p.y)))
    }
    override func mouseMoved(with event:NSEvent) {
        pointer=eventPoint(event);selector?.activate(self)
        if selector?.toolbar.frame.contains(pointer)==true {return}
        selector?.hover(at:NSPoint(x:pointer.x+screen.frame.minX,y:pointer.y+screen.frame.minY),on:screen)
    }
    override func mouseDown(with event:NSEvent) {
        guard let selector=selector else{return};selector.activate(self)
        pointer=eventPoint(event)
        if selector.mode != .region {
            selector.hover(at:NSPoint(x:pointer.x+screen.frame.minX,y:pointer.y+screen.frame.minY),on:screen)
            selector.complete();return
        }
        start=pointer;original=localSelection ?? .zero;drag="new"
        if !original.isEmpty {
            if let i=handles(original).firstIndex(where:{hypot($0.x-pointer.x,$0.y-pointer.y)<9}){drag="handle\(i)"}
            else if original.contains(pointer){drag="move"}
        }
        if drag=="new" {selector.region=nil;selector.redraw()}
    }
    override func mouseDragged(with event:NSEvent) {
        guard let selector=selector,selector.mode == .region else{return}
        pointer=eventPoint(event);var rect=NSRect.zero
        if drag=="move" {rect=boundedRect(original.offsetBy(dx:pointer.x-start.x,dy:pointer.y-start.y),in:bounds)}
        else if drag.hasPrefix("handle"),let i=Int(drag.dropFirst(6)) {
            var a=NSPoint(x:original.minX,y:original.minY),b=NSPoint(x:original.maxX,y:original.maxY)
            if [0,3,5].contains(i){a.x=pointer.x};if [2,4,7].contains(i){b.x=pointer.x}
            if [0,1,2].contains(i){a.y=pointer.y};if [5,6,7].contains(i){b.y=pointer.y}
            rect=regionBetween(a,b,in:bounds)
        } else {rect=regionBetween(start,pointer,in:bounds)}
        selector.region=rect.offsetBy(dx:screen.frame.minX,dy:screen.frame.minY);selector.redraw()
    }
    override func mouseUp(with event:NSEvent) {
        if !drag.isEmpty {mouseDragged(with:event)}
        drag="";selector?.redraw()
    }
    override func rightMouseDown(with event:NSEvent){selector?.cancel()}
}

final class ScreenshotSelector: NSObject {
    var mode:CaptureMode
    let english:Bool
    let screens:[NSScreen]
    let primaryTop:CGFloat
    var windows:[SelectionWindow]=[]
    var views:[SelectionView]=[]
    var targets:[CaptureTarget]=[]
    var region:NSRect?
    var hovered:CaptureTarget?
    var activeScreen:NSScreen
    weak var activeView:SelectionView?
    var result:[String:Any]=["cancelled":true]
    var finished=false
    let toolbar=NSVisualEffectView(frame:NSRect(x:0,y:0,width:564,height:94))
    let hint=NSTextField(labelWithString:"")
    var modeButtons:[CaptureMode:NSButton]=[:]
    let confirm=NSButton(title:"",target:nil,action:nil)
    var selectedRect:NSRect? {
        switch mode {case .region:return region;case .window:return hovered?.frame;case .screen:return activeScreen.frame}
    }
    init(mode:CaptureMode,english:Bool,screens:[NSScreen]=NSScreen.screens) {
        self.mode=mode;self.english=english;self.screens=screens
        primaryTop=screens[0].frame.maxY;activeScreen=screens[0]
        super.init();configureToolbar()
    }
    func l(_ zh:String,_ en:String)->String {english ? en:zh}
    func configureToolbar() {
        toolbar.material = .hudWindow;toolbar.blendingMode = .withinWindow;toolbar.state = .active
        toolbar.appearance=NSAppearance(named:.darkAqua);toolbar.wantsLayer=true
        toolbar.layer?.cornerRadius=14;toolbar.layer?.masksToBounds=true
        let stack=NSStackView();stack.orientation = .horizontal;stack.spacing=8
        stack.frame=NSRect(x:14,y:42,width:536,height:36);toolbar.addSubview(stack)
        for (i,m) in CaptureMode.allCases.enumerated() {
            let names=[l("区域","Region"),l("窗口","Window"),l("全屏","Display")]
            let b=NSButton(title:"\(names[i])  \(i+1)",target:self,action:#selector(modeClicked(_:)))
            b.tag=i;b.setButtonType(.pushOnPushOff);b.bezelStyle = .rounded;b.font = .systemFont(ofSize:13,weight:.medium)
            b.toolTip=l("直接切换截图方式，无需退出截图","Switch modes without leaving capture")
            b.widthAnchor.constraint(equalToConstant:92).isActive=true;modeButtons[m]=b;stack.addArrangedSubview(b)
        }
        let spacer=NSView();spacer.widthAnchor.constraint(equalToConstant:4).isActive=true;stack.addArrangedSubview(spacer)
        let cancel=NSButton(title:l("取消 Esc","Cancel Esc"),target:self,action:#selector(cancelClicked))
        cancel.bezelStyle = .rounded;cancel.widthAnchor.constraint(equalToConstant:94).isActive=true;stack.addArrangedSubview(cancel)
        confirm.title=l("截取 ↵","Capture ↵");confirm.target=self;confirm.action=#selector(confirmClicked)
        confirm.bezelStyle = .rounded;confirm.bezelColor = .systemIndigo;confirm.widthAnchor.constraint(equalToConstant:112).isActive=true;stack.addArrangedSubview(confirm)
        hint.frame=NSRect(x:12,y:13,width:540,height:18);hint.alignment = .center;hint.font = .systemFont(ofSize:11)
        hint.textColor = .secondaryLabelColor;toolbar.addSubview(hint)
    }
    func open(show:Bool=true) {
        let raw=CGWindowListCopyWindowInfo([.optionOnScreenOnly,.excludeDesktopElements],kCGNullWindowID) as? [[String:Any]] ?? []
        targets=raw.compactMap{info in
            guard let id=info[kCGWindowNumber as String] as? UInt32,let layer=info[kCGWindowLayer as String] as? Int,layer==0,
                  let pid=info[kCGWindowOwnerPID as String] as? Int32,pid != getpid(),pid != getppid(),
                  let dict=info[kCGWindowBounds as String] as? [String:Any],let frame=CGRect(dictionaryRepresentation:dict as CFDictionary),frame.width>=40,frame.height>=30,
                  (info[kCGWindowAlpha as String] as? Double ?? 1)>0.01 else{return nil}
            return CaptureTarget(id:id,frame:cocoaRect(frame,primaryTop:primaryTop),title:info[kCGWindowOwnerName as String] as? String ?? "")
        }
        for screen in screens {
            let w=SelectionWindow(contentRect:screen.frame,styleMask:.borderless,backing:.buffered,defer:false)
            w.selector=self;w.isReleasedWhenClosed=false;w.isOpaque=false;w.backgroundColor = .clear;w.hasShadow=false
            w.level = .screenSaver;w.collectionBehavior=[.canJoinAllSpaces,.fullScreenAuxiliary,.stationary]
            w.acceptsMouseMovedEvents=true;w.title="PasteNext Capture Selection"
            let view=SelectionView(screen:screen,selector:self);w.contentView=view;views.append(view);windows.append(w)
            if show {w.orderFrontRegardless()}
        }
        let mouse=NSEvent.mouseLocation
        let first=views.first(where:{$0.screen.frame.contains(mouse)}) ?? views[0]
        activate(first);hover(at:mouse,on:first.screen)
        if show {NSApp.activate(ignoringOtherApps:true);first.window?.makeKeyAndOrderFront(nil);NSCursor.crosshair.set()}
    }
    func activate(_ view:SelectionView) {
        guard activeView !== view else{return}
        activeView=view;activeScreen=view.screen
        toolbar.removeFromSuperview();view.addSubview(toolbar)
        let bottom=view.screen.visibleFrame.minY-view.screen.frame.minY+24
        toolbar.setFrameOrigin(NSPoint(x:max(8,(view.bounds.width-toolbar.frame.width)/2),y:max(24,bottom)))
        view.window?.makeFirstResponder(view);redraw()
    }
    func hover(at point:NSPoint,on screen:NSScreen) {
        if mode == .window {hovered=targets.first(where:{$0.frame.contains(point)})}
        if mode == .screen {activeScreen=screen}
        redraw()
    }
    func switchMode(_ next:CaptureMode) {
        mode=next;region=nil;hovered=nil
        if let view=activeView {let p=view.pointer;hover(at:NSPoint(x:p.x+view.screen.frame.minX,y:p.y+view.screen.frame.minY),on:view.screen)}
        redraw()
    }
    func redraw() {
        for (m,b) in modeButtons {b.state=m==mode ? .on:.off}
        confirm.isEnabled=selectedRect.map{$0.width>=2 && $0.height>=2} ?? false
        hint.stringValue=mode == .region ? l("拖动框选 · 拖动边角调整 · 1/2/3 切换 · 回车截取","Drag to select or resize · 1/2/3 switch modes · Enter captures"):
            mode == .window ? l("移动鼠标选窗口，单击截取 · 1/2/3 切换","Point to a window and click to capture · 1/2/3 switch modes"):
            l("单击目标屏幕截取 · 1/2/3 切换 · Esc 取消","Click a display to capture · 1/2/3 switch modes · Esc cancels")
        views.forEach{$0.needsDisplay=true}
    }
    @objc func modeClicked(_ sender:NSButton){switchMode(CaptureMode.allCases[sender.tag]);activeView?.window?.makeFirstResponder(activeView)}
    @objc func cancelClicked(){cancel()}
    @objc func confirmClicked(){complete()}
    func keyDown(_ event:NSEvent) {
        if event.keyCode==53 {cancel();return}
        if event.modifierFlags.intersection([.command,.control,.option]).isEmpty {
            switch event.keyCode {
            case 18:switchMode(.region)
            case 19:switchMode(.window)
            case 20:switchMode(.screen)
            case 48,49:
                let modes=CaptureMode.allCases;let i=modes.firstIndex(of:mode)!;switchMode(modes[(i+1)%modes.count])
            case 36,76:complete()
            default:break
            }
        }
    }
    func complete() {
        guard !finished,let rect=selectedRect,rect.width>=2,rect.height>=2 else{return}
        let q=quartzRect(rect,primaryTop:primaryTop).integral
        var output:[String:Any]=["cancelled":false,"mode":mode.rawValue,"display":(screens.firstIndex(of:activeScreen) ?? 0)+1,
            "rect":["x":q.minX,"y":q.minY,"width":q.width,"height":q.height]]
        if mode == .window {guard let id=hovered?.id else{return};output["windowId"]=id}
        finish(output)
    }
    func cancel(){finish(["cancelled":true])}
    func finish(_ output:[String:Any]) {
        guard !finished else{return};finished=true;result=output
        windows.forEach{$0.orderOut(nil)};NSApp.stop(nil)
        if let event=NSEvent.otherEvent(with:.applicationDefined,location:.zero,modifierFlags:[],timestamp:0,windowNumber:0,context:nil,subtype:0,data1:0,data2:0){NSApp.postEvent(event,atStart:false)}
    }
}

#if !CAPTURE_TESTS
@main struct CaptureSelectorMain {
    static func main() throws {
let arguments=CommandLine.arguments
func argument(_ key:String)->String? {guard let i=arguments.firstIndex(of:key),arguments.count>i+1 else{return nil};return arguments[i+1]}
let app=NSApplication.shared
app.setActivationPolicy(.accessory)
guard !NSScreen.screens.isEmpty else{fputs("No active display\n",stderr);exit(1)}
let selector=ScreenshotSelector(mode:CaptureMode(rawValue:argument("--mode") ?? "region") ?? .region,english:argument("--locale")=="en")
selector.open();app.run()
let json=try JSONSerialization.data(withJSONObject:selector.result,options:[.sortedKeys])
FileHandle.standardOutput.write(json);FileHandle.standardOutput.write(Data([10]))
    }
}
#endif
