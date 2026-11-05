// Original Win32 selector. Physical screen coordinates also support negative monitor origins.
#define NOMINMAX
#include <windows.h>
#include <windowsx.h>
#include <dwmapi.h>
#include <shellscalingapi.h>
#include <algorithm>
#include <cmath>
#include <iostream>
#include <string>
#include <vector>

struct Target { HWND handle; RECT rect; };
struct Monitor { RECT rect; bool primary; double scale; };
static RECT desktop{}, region{};
static std::vector<Target> targets;
static std::vector<Monitor> monitors;
static HWND overlay=nullptr;
static HDC snapshot=nullptr, shadedSnapshot=nullptr, drawing=nullptr;
static HBITMAP shadedBitmap=nullptr,drawingBitmap=nullptr;
static HGDIOBJ oldShaded=nullptr,oldDrawing=nullptr;
static HBITMAP bitmap=nullptr;
static HGDIOBJ oldBitmap=nullptr;
static int mode=0, monitorIndex=0, hoverIndex=-1, drag=-1;
static POINT start{};
static RECT original{};
static bool english=false, finished=false, video=false;
static std::string output="{\"cancelled\":true}";
static HFONT font=nullptr;
static std::vector<HFONT> fonts;
static RECT toolbar{}, buttons[5]{};
static HANDLE recordProcess=nullptr;
static ULONGLONG lastClick=0;
static bool wasPressed=false;
static const wchar_t* text(const wchar_t* zh,const wchar_t* en){return english?en:zh;}
static RECT local(RECT r){OffsetRect(&r,-desktop.left,-desktop.top);return r;}
static RECT chosen(){return mode==0?region:mode==1?(hoverIndex>=0?targets[hoverIndex].rect:RECT{}):monitors[monitorIndex].rect;}
static bool valid(RECT r){return r.right-r.left>=2&&r.bottom-r.top>=2;}
static RECT between(POINT a,POINT b){return RECT{std::min(a.x,b.x),std::min(a.y,b.y),std::max<LONG>(a.x,b.x),std::max<LONG>(a.y,b.y)};}
static POINT clampPoint(POINT p){p.x=std::clamp<LONG>(p.x,desktop.left,desktop.right);p.y=std::clamp<LONG>(p.y,desktop.top,desktop.bottom);return p;}
static POINT point(LPARAM l){return clampPoint(POINT{GET_X_LPARAM(l)+desktop.left,GET_Y_LPARAM(l)+desktop.top});}
static void cancel(){finished=true;DestroyWindow(overlay);}
static void confirm(){
    RECT r=chosen();if(!valid(r))return;
    const char* name=mode==0?"region":mode==1?"window":"screen";
    output="{\"cancelled\":false,\"mode\":\""+std::string(name)+"\",\"display\":"+std::to_string(monitorIndex+1)+",\"scale\":"+std::to_string(monitors[monitorIndex].scale)+",\"rect\":{\"x\":"+std::to_string(r.left)+",\"y\":"+std::to_string(r.top)+",\"width\":"+std::to_string(r.right-r.left)+",\"height\":"+std::to_string(r.bottom-r.top)+"}";
    if(mode==1)output+=",\"windowId\":"+std::to_string(reinterpret_cast<uintptr_t>(targets[hoverIndex].handle));
    output+="}";finished=true;DestroyWindow(overlay);
}
static void hover(POINT p){
    for(size_t i=0;i<monitors.size();++i)if(PtInRect(&monitors[i].rect,p)){monitorIndex=static_cast<int>(i);break;}
    hoverIndex=-1;
    if(mode==1)for(size_t i=0;i<targets.size();++i)if(PtInRect(&targets[i].rect,p)){hoverIndex=static_cast<int>(i);break;}
}
static void switchMode(int next){mode=next;region={};drag=-1;ReleaseCapture();POINT p{};GetCursorPos(&p);hover(p);InvalidateRect(overlay,nullptr,FALSE);}
static void box(HDC dc,RECT r,COLORREF color,int radius=0){HBRUSH b=CreateSolidBrush(color);HGDIOBJ old=SelectObject(dc,b);HGDIOBJ pen=SelectObject(dc,GetStockObject(NULL_PEN));if(radius)RoundRect(dc,r.left,r.top,r.right,r.bottom,radius,radius);else Rectangle(dc,r.left,r.top,r.right,r.bottom);SelectObject(dc,old);SelectObject(dc,pen);DeleteObject(b);}
static void label(HDC dc,RECT r,const wchar_t* value,COLORREF color=RGB(245,245,250)){SetTextColor(dc,color);DrawTextW(dc,value,-1,&r,DT_CENTER|DT_VCENTER|DT_SINGLELINE);}
static std::vector<POINT> handles(RECT r){return {{r.left,r.top},{(r.left+r.right)/2,r.top},{r.right,r.top},{r.left,(r.top+r.bottom)/2},{r.right,(r.top+r.bottom)/2},{r.left,r.bottom},{(r.left+r.right)/2,r.bottom},{r.right,r.bottom}};}
static void paint(HWND hwnd){
    PAINTSTRUCT ps;HDC dc=BeginPaint(hwnd,&ps);int w=desktop.right-desktop.left,h=desktop.bottom-desktop.top;
    const double scale=monitors[monitorIndex].scale;auto ui=[scale](int value)->LONG{return static_cast<LONG>(std::lround(value*scale));};font=fonts[monitorIndex];
    HDC buffer=drawing;
    BitBlt(buffer,0,0,w,h,shadedSnapshot,0,0,SRCCOPY);
    RECT r=chosen();if(valid(r)){
        RECT draw=local(r);RECT clipped{};RECT bounds{0,0,w,h};IntersectRect(&clipped,&draw,&bounds);
        BitBlt(buffer,clipped.left,clipped.top,clipped.right-clipped.left,clipped.bottom-clipped.top,snapshot,clipped.left,clipped.top,SRCCOPY);
        HPEN p=CreatePen(PS_SOLID,ui(2),RGB(157,157,255));HGDIOBJ oldp=SelectObject(buffer,p);HGDIOBJ oldb=SelectObject(buffer,GetStockObject(HOLLOW_BRUSH));Rectangle(buffer,draw.left,draw.top,draw.right,draw.bottom);SelectObject(buffer,oldp);SelectObject(buffer,oldb);DeleteObject(p);
        if(mode==0)for(auto pt:handles(draw))box(buffer,{pt.x-ui(3),pt.y-ui(3),pt.x+ui(4),pt.y+ui(4)},RGB(255,255,255));
        RECT dim{std::clamp<LONG>(draw.left,ui(8),std::max<LONG>(ui(8),w-ui(188))),std::clamp<LONG>(draw.top-ui(34),ui(8),std::max<LONG>(ui(8),h-ui(36))),0,0};dim.right=dim.left+ui(180);dim.bottom=dim.top+ui(28);box(buffer,dim,RGB(22,24,34),ui(8));
        SelectObject(buffer,font);SetBkMode(buffer,TRANSPARENT);std::wstring size=std::to_wstring(r.right-r.left)+L" × "+std::to_wstring(r.bottom-r.top)+L" px";label(buffer,dim,size.c_str());
    }
    RECT display=local(monitors[monitorIndex].rect);
    int left=display.left+std::max<LONG>(ui(8),(display.right-display.left-ui(570))/2),top=display.bottom-ui(124);
    toolbar={left,top,left+ui(570),top+ui(96)};box(buffer,toolbar,RGB(28,30,42),ui(16));SelectObject(buffer,font);SetBkMode(buffer,TRANSPARENT);
    for(int i=0;i<5;++i){int width=ui(i<3?92:i==3?98:112);int x=left+ui(14+(i<3?i*100:300+(i-3)*106));buttons[i]={x,top+ui(12),x+width,top+ui(48)};
        box(buffer,buttons[i],i==mode?RGB(89,85,183):i==4?RGB(91,83,201):RGB(49,52,68),ui(8));
        const wchar_t* values[]={text(L"区域  1",L"Region  1"),text(L"窗口  2",L"Window  2"),text(L"全屏  3",L"Display  3"),text(L"取消 Esc",L"Cancel Esc"),video?text(L"录制 ↵",L"Record ↵"):text(L"截取 ↵",L"Capture ↵")};label(buffer,buttons[i],values[i],i==4&&!valid(r)?RGB(145,147,164):RGB(255,255,255));
    }
    RECT hint{left+ui(8),top+ui(57),left+ui(562),top+ui(88)};label(buffer,hint,mode==0?text(L"拖动框选或调整边角 · 1/2/3 切换 · 回车截取",L"Drag to select / resize · 1/2/3 switch modes · Enter captures"):mode==1?text(L"移动鼠标选窗口，单击截取 · 1/2/3 切换",L"Point to a window and click · 1/2/3 switch modes"):text(L"单击目标屏幕截取 · 1/2/3 切换 · Esc 取消",L"Click a display · 1/2/3 switch modes · Esc cancels"),RGB(185,188,205));
    BitBlt(dc,0,0,w,h,buffer,0,0,SRCCOPY);EndPaint(hwnd,&ps);
}
static LRESULT CALLBACK procedure(HWND hwnd,UINT message,WPARAM key,LPARAM data){
    switch(message){
    case WM_PAINT:paint(hwnd);return 0;
    case WM_ERASEBKGND:return 1;
    case WM_KEYDOWN:
        if(key==VK_ESCAPE){cancel();return 0;}if(key==VK_RETURN){confirm();return 0;}
        if(key>='1'&&key<='3'){switchMode(static_cast<int>(key-'1'));return 0;}if(key==VK_SPACE||key==VK_TAB){switchMode((mode+1)%3);return 0;}break;
    case WM_LBUTTONDOWN:{
        POINT pt{GET_X_LPARAM(data),GET_Y_LPARAM(data)};for(int i=0;i<5;++i)if(PtInRect(&buttons[i],pt)){if(i<3)switchMode(i);else if(i==3)cancel();else confirm();return 0;}
        POINT p=point(data);hover(p);if(mode!=0){confirm();return 0;}
        start=p;original=region;drag=0;if(valid(region)){
            auto hs=handles(region);for(int i=0;i<8;++i)if(std::abs(hs[i].x-p.x)<9*monitors[monitorIndex].scale&&std::abs(hs[i].y-p.y)<9*monitors[monitorIndex].scale){drag=i+2;break;}
            if(drag==0&&PtInRect(&region,p))drag=1;
        }if(drag==0)region={};SetCapture(hwnd);InvalidateRect(hwnd,nullptr,FALSE);return 0;}
    case WM_MOUSEMOVE:{
        POINT p=point(data);if(drag>=0){
            if(drag==0)region=between(start,p);
            else if(drag==1){region=original;LONG dx=std::clamp<LONG>(p.x-start.x,desktop.left-original.left,desktop.right-original.right),dy=std::clamp<LONG>(p.y-start.y,desktop.top-original.top,desktop.bottom-original.bottom);OffsetRect(&region,dx,dy);}
            else {int i=drag-2;POINT a{original.left,original.top},b{original.right,original.bottom};if(i==0||i==3||i==5)a.x=p.x;if(i==2||i==4||i==7)b.x=p.x;if(i<=2)a.y=p.y;if(i>=5)b.y=p.y;region=between(a,b);}
        }else {POINT client{GET_X_LPARAM(data),GET_Y_LPARAM(data)};if(!PtInRect(&toolbar,client))hover(p);}InvalidateRect(hwnd,nullptr,FALSE);return 0;}
    case WM_LBUTTONUP:drag=-1;ReleaseCapture();return 0;
    case WM_RBUTTONDOWN:cancel();return 0;
    case WM_CLOSE:cancel();return 0;
    case WM_DESTROY:PostQuitMessage(0);return 0;
    }return DefWindowProcW(hwnd,message,key,data);
}
static BOOL CALLBACK enumerate(HWND hwnd,LPARAM){
    DWORD pid=0;GetWindowThreadProcessId(hwnd,&pid);
    if(!IsWindowVisible(hwnd)||IsIconic(hwnd)||pid==GetCurrentProcessId())return TRUE;
    BOOL cloaked=FALSE;DwmGetWindowAttribute(hwnd,DWMWA_CLOAKED,&cloaked,sizeof(cloaked));if(cloaked)return TRUE;
    RECT r{};if(FAILED(DwmGetWindowAttribute(hwnd,DWMWA_EXTENDED_FRAME_BOUNDS,&r,sizeof(r))))GetWindowRect(hwnd,&r);
    if(r.right-r.left>=40&&r.bottom-r.top>=30)targets.push_back({hwnd,r});return TRUE;
}
static BOOL CALLBACK enumerateMonitor(HMONITOR h,HDC,LPRECT,LPARAM){MONITORINFO i{};i.cbSize=sizeof(i);GetMonitorInfoW(h,&i);UINT x=96,y=96;GetDpiForMonitor(h,MDT_EFFECTIVE_DPI,&x,&y);monitors.push_back({i.rcMonitor,(i.dwFlags&MONITORINFOF_PRIMARY)!=0,x/96.0});return TRUE;}
static LRESULT CALLBACK clickProcedure(HWND hwnd,UINT msg,WPARAM key,LPARAM data){
    if(msg==WM_TIMER){DWORD status=0;if(!GetExitCodeProcess(recordProcess,&status)||status!=STILL_ACTIVE){DestroyWindow(hwnd);return 0;}
        bool pressed=(GetAsyncKeyState(VK_LBUTTON)&0x8000)||(GetAsyncKeyState(VK_RBUTTON)&0x8000);
        if(pressed&&!wasPressed){POINT p{};GetCursorPos(&p);lastClick=GetTickCount64();SetWindowPos(hwnd,HWND_TOPMOST,p.x-24,p.y-24,48,48,SWP_NOACTIVATE|SWP_SHOWWINDOW);}
        wasPressed=pressed;if(lastClick&&GetTickCount64()-lastClick<550){InvalidateRect(hwnd,nullptr,FALSE);}else ShowWindow(hwnd,SW_HIDE);return 0;
    }
    if(msg==WM_PAINT){PAINTSTRUCT p;HDC dc=BeginPaint(hwnd,&p);box(dc,{0,0,48,48},RGB(255,0,255));HPEN pen=CreatePen(PS_SOLID,3,RGB(255,190,40));HGDIOBJ old=SelectObject(dc,pen);HGDIOBJ brush=SelectObject(dc,GetStockObject(HOLLOW_BRUSH));Ellipse(dc,5,5,43,43);SelectObject(dc,old);SelectObject(dc,brush);DeleteObject(pen);EndPaint(hwnd,&p);return 0;}
    if(msg==WM_DESTROY){PostQuitMessage(0);return 0;}return DefWindowProcW(hwnd,msg,key,data);
}
int wmain(int argc,wchar_t** argv){
    SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    for(int i=1;i+1<argc;++i){std::wstring a=argv[i];if(a==L"--mode")mode=std::wstring(argv[i+1])==L"window"?1:std::wstring(argv[i+1])==L"screen"?2:0;if(a==L"--locale")english=std::wstring(argv[i+1])==L"en";if(a==L"--video")video=std::wstring(argv[i+1])==L"1";}
    if(argc>1&&std::wstring(argv[1])==L"--beep"){MessageBeep(MB_OK);return 0;}
    HINSTANCE instance=GetModuleHandleW(nullptr);WNDCLASSW wc{};wc.hInstance=instance;wc.hCursor=LoadCursorW(nullptr,IDC_CROSS);wc.lpszClassName=L"PasteNextCaptureSelector";wc.lpfnWndProc=procedure;
    if(argc>2&&std::wstring(argv[1])==L"--clicks"){
        recordProcess=OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION,FALSE,std::stoul(argv[2]));if(!recordProcess)return 0;
        wc.lpfnWndProc=clickProcedure;RegisterClassW(&wc);overlay=CreateWindowExW(WS_EX_LAYERED|WS_EX_TRANSPARENT|WS_EX_TOOLWINDOW|WS_EX_NOACTIVATE,wc.lpszClassName,L"PasteNext Click Indicator",WS_POPUP,0,0,48,48,nullptr,nullptr,instance,nullptr);SetLayeredWindowAttributes(overlay,RGB(255,0,255),255,LWA_COLORKEY);SetTimer(overlay,1,20,nullptr);
    }else{
        desktop={GetSystemMetrics(SM_XVIRTUALSCREEN),GetSystemMetrics(SM_YVIRTUALSCREEN),0,0};desktop.right=desktop.left+GetSystemMetrics(SM_CXVIRTUALSCREEN);desktop.bottom=desktop.top+GetSystemMetrics(SM_CYVIRTUALSCREEN);
        int w=desktop.right-desktop.left,h=desktop.bottom-desktop.top;if(w<=0||h<=0||static_cast<long long>(w)*h>128000000){std::cerr<<"Display dimensions are unsupported";return 1;}
        EnumDisplayMonitors(nullptr,nullptr,enumerateMonitor,0);if(monitors.empty())return 1;std::stable_sort(monitors.begin(),monitors.end(),[](const Monitor&a,const Monitor&b){return a.primary&&!b.primary;});EnumWindows(enumerate,0);
        HDC screen=GetDC(nullptr);snapshot=CreateCompatibleDC(screen);bitmap=CreateCompatibleBitmap(screen,w,h);oldBitmap=SelectObject(snapshot,bitmap);BitBlt(snapshot,0,0,w,h,screen,desktop.left,desktop.top,SRCCOPY|CAPTUREBLT);shadedSnapshot=CreateCompatibleDC(screen);shadedBitmap=CreateCompatibleBitmap(screen,w,h);oldShaded=SelectObject(shadedSnapshot,shadedBitmap);BitBlt(shadedSnapshot,0,0,w,h,snapshot,0,0,SRCCOPY);
        HDC shade=CreateCompatibleDC(screen);HBITMAP black=CreateCompatibleBitmap(screen,1,1);auto old=SelectObject(shade,black);box(shade,{0,0,1,1},RGB(0,0,0));BLENDFUNCTION blend{AC_SRC_OVER,0,153,0};AlphaBlend(shadedSnapshot,0,0,w,h,shade,0,0,1,1,blend);SelectObject(shade,old);DeleteObject(black);DeleteDC(shade);
        drawing=CreateCompatibleDC(screen);drawingBitmap=CreateCompatibleBitmap(screen,w,h);oldDrawing=SelectObject(drawing,drawingBitmap);ReleaseDC(nullptr,screen);
        for(const auto& monitor:monitors)fonts.push_back(CreateFontW(-static_cast<int>(std::lround(15*monitor.scale)),0,0,0,FW_MEDIUM,FALSE,FALSE,FALSE,DEFAULT_CHARSET,OUT_DEFAULT_PRECIS,CLIP_DEFAULT_PRECIS,CLEARTYPE_QUALITY,DEFAULT_PITCH,L"Segoe UI"));font=fonts[0];RegisterClassW(&wc);
        overlay=CreateWindowExW(WS_EX_TOPMOST|WS_EX_TOOLWINDOW,wc.lpszClassName,L"PasteNext Capture Selection",WS_POPUP,desktop.left,desktop.top,w,h,nullptr,nullptr,instance,nullptr);
        if(!overlay){std::cerr<<"Unable to open selection overlay";return 1;}POINT mouse{};GetCursorPos(&mouse);hover(mouse);ShowWindow(overlay,SW_SHOW);SetForegroundWindow(overlay);SetFocus(overlay);
        if(argc>1&&std::wstring(argv[1])==L"--self-test"){
            SendMessageW(overlay,WM_KEYDOWN,'2',0);if(mode!=1)return 2;SendMessageW(overlay,WM_KEYDOWN,'3',0);if(mode!=2)return 3;SendMessageW(overlay,WM_KEYDOWN,'1',0);
            SendMessageW(overlay,WM_LBUTTONDOWN,MK_LBUTTON,MAKELPARAM(60,60));SendMessageW(overlay,WM_MOUSEMOVE,MK_LBUTTON,MAKELPARAM(240,180));SendMessageW(overlay,WM_LBUTTONUP,0,MAKELPARAM(240,180));
            if(region.right-region.left!=180||region.bottom-region.top!=120)return 4;
            SendMessageW(overlay,WM_LBUTTONDOWN,MK_LBUTTON,MAKELPARAM(100,100));SendMessageW(overlay,WM_MOUSEMOVE,MK_LBUTTON,MAKELPARAM(120,110));SendMessageW(overlay,WM_LBUTTONUP,0,MAKELPARAM(120,110));
            if(region.left!=desktop.left+80||region.top!=desktop.top+70)return 5;
            SendMessageW(overlay,WM_KEYDOWN,VK_RETURN,0);if(output.find("\"cancelled\":false")==std::string::npos)return 6;
        }

    }
    MSG message;while(GetMessageW(&message,nullptr,0,0)>0){TranslateMessage(&message);DispatchMessageW(&message);}
    if(recordProcess){CloseHandle(recordProcess);return 0;}
    if(drawing){SelectObject(drawing,oldDrawing);DeleteObject(drawingBitmap);DeleteDC(drawing);}if(shadedSnapshot){SelectObject(shadedSnapshot,oldShaded);DeleteObject(shadedBitmap);DeleteDC(shadedSnapshot);}
    if(snapshot){SelectObject(snapshot,oldBitmap);DeleteObject(bitmap);DeleteDC(snapshot);}for(auto f:fonts)if(f)DeleteObject(f);
    std::cout<<output<<std::endl;return 0;
}
