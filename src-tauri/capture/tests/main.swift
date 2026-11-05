import AppKit
import CoreGraphics
func check(_ condition: @autoclosure ()->Bool,_ message:String){if !condition(){fputs("FAILED: \(message)\n",stderr);exit(1)}}
check(quartzRect(NSRect(x:-400,y:100,width:200,height:150),primaryTop:900)==CGRect(x:-400,y:650,width:200,height:150),"negative monitor coordinates")
let above=NSRect(x:0,y:1000,width:100,height:200)
check(cocoaRect(quartzRect(above,primaryTop:900),primaryTop:900)==above,"display above primary round trip")
check(regionBetween(NSPoint(x:150,y:100),NSPoint(x:-20,y:-30),in:NSRect(x:0,y:0,width:300,height:200))==NSRect(x:0,y:0,width:150,height:100),"reverse drag clipped to display")
check(boundedRect(NSRect(x:280,y:190,width:80,height:60),in:NSRect(x:0,y:0,width:300,height:200))==NSRect(x:220,y:140,width:80,height:60),"move preserves dimensions at edge")
let app=NSApplication.shared;app.setActivationPolicy(.accessory)
let controller=ScreenshotSelector(mode:.region,english:false);controller.open(show:false)
let view=controller.views[0];controller.activate(view)
func mouse(_ type:NSEvent.EventType,_ x:CGFloat,_ y:CGFloat)->NSEvent{NSEvent.mouseEvent(with:type,location:NSPoint(x:x,y:y),modifierFlags:[],timestamp:0,windowNumber:view.window!.windowNumber,context:nil,eventNumber:1,clickCount:1,pressure:1)!}
view.mouseDown(with:mouse(.leftMouseDown,100,100));view.mouseDragged(with:mouse(.leftMouseDragged,400,300));view.mouseUp(with:mouse(.leftMouseUp,400,300))
let origin=view.screen.frame.origin
check(controller.region==NSRect(x:origin.x+100,y:origin.y+100,width:300,height:200),"native region drag")
view.mouseDown(with:mouse(.leftMouseDown,200,200));view.mouseDragged(with:mouse(.leftMouseDragged,250,230));view.mouseUp(with:mouse(.leftMouseUp,250,230))
check(controller.region==NSRect(x:origin.x+150,y:origin.y+130,width:300,height:200),"native selection move")
view.mouseDown(with:mouse(.leftMouseDown,450,330));view.mouseDragged(with:mouse(.leftMouseDragged,500,360));view.mouseUp(with:mouse(.leftMouseUp,500,360))
check(controller.region==NSRect(x:origin.x+150,y:origin.y+130,width:350,height:230),"native handle resize")
controller.modeButtons[.window]!.performClick(nil);check(controller.mode == .window && controller.region == nil,"toolbar switches to window mode without leaving capture")
controller.modeButtons[.screen]!.performClick(nil);check(controller.mode == .screen && controller.selectedRect==view.screen.frame,"toolbar switches to display")
controller.keyDown(NSEvent.keyEvent(with:.keyDown,location:.zero,modifierFlags:[],timestamp:0,windowNumber:view.window!.windowNumber,context:nil,characters:"1",charactersIgnoringModifiers:"1",isARepeat:false,keyCode:18)!)
check(controller.mode == .region,"keyboard switches to region")
view.setFrameSize(NSSize(width:400,height:300));controller.region=NSRect(x:origin.x+100,y:origin.y+100,width:100,height:100)
let bitmap=view.bitmapImageRepForCachingDisplay(in:view.bounds)!;view.cacheDisplay(in:view.bounds,to:bitmap)
let pixelScale=CGFloat(bitmap.pixelsWide)/view.bounds.width
let outside=bitmap.colorAt(x:Int(10*pixelScale),y:Int(10*pixelScale))!,inside=bitmap.colorAt(x:Int(150*pixelScale),y:Int(150*pixelScale))!
check(abs(outside.alphaComponent-selectionShade)<0.04,"outside selection uses a 60 percent dim mask")
check(inside.alphaComponent<0.02,"selected area stays clear")
if let i=CommandLine.arguments.firstIndex(of:"--snapshot"),CommandLine.arguments.count>i+1{try bitmap.representation(using:.png,properties:[:])!.write(to:URL(fileURLWithPath:CommandLine.arguments[i+1]))}
controller.complete();check(controller.result["cancelled"] as? Bool == false,"confirmation produces a capture selection")
print("macOS native selection: geometry, mode buttons, keyboard, move/resize, mask rendering and confirmation passed")
