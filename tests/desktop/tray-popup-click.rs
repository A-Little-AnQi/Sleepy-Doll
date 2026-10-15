#![cfg(target_os = "windows")]
#![allow(dead_code)]
use rust_embed::RustEmbed;
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
use tao::{
    event::{Event, StartCause, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder},
    platform::{
        run_return::EventLoopExtRunReturn,
        windows::{EventLoopBuilderExtWindows, WindowExtWindows},
    },
};
#[derive(Debug)]
pub enum UserEvent {
    TrayAction(Value),
}
#[derive(RustEmbed)]
#[folder = "target/ui/"]
pub struct UiAssets;
#[path = "../../src/tray_popup.rs"]
mod tray_popup;
#[path = "../../src/tray_request.rs"]
mod tray_request;

#[test]
#[ignore = "需要隔离的 WebView2 缓存和 Windows 桌面"]
fn embedded_webview_click_reaches_action_without_parent_dismissal() {
    use windows_sys::Win32::{
        Foundation::POINT,
        Graphics::Gdi::{ClientToScreen, ScreenToClient},
        UI::Input::KeyboardAndMouse::SetFocus,
        UI::WindowsAndMessaging::{
            IsChild, PostMessageW, SendMessageW, WM_KILLFOCUS, WM_LBUTTONDOWN, WM_LBUTTONUP,
            WM_MOUSEMOVE, WM_SETFOCUS, WindowFromPoint,
        },
    };
    let cache =
        PathBuf::from(std::env::var_os("SLEEPY_DOLL_WINDOW_VALIDATION").expect("缺少隔离缓存路径"));
    assert!(cache.is_dir(), "复用已登记的隔离缓存，不创建替代目录");
    let mut events = EventLoopBuilder::<UserEvent>::with_user_event()
        .with_any_thread(true)
        .build();
    let mut popup = tray_popup::TrayPopup::new_for_validation(
        &events,
        events.create_proxy(),
        cache,
        json!({"dark":true,"bridge":false,"active":true}),
    )
    .unwrap();
    popup.show(
        600.,
        600.,
        json!({"dark":true,"bridge":false,"active":true}),
    );
    let deadline = Instant::now() + Duration::from_secs(20);
    let expected = [
        "open", "settings", "help", "folder", "bridge", "stop", "quit",
    ];
    let mut clicked = Vec::new();
    let mut internal_focus_events = 0;
    let mut internal_focus_preserved = false;
    let mut measured_border = false;
    events.run_return(|event,_,control|{
        *control=ControlFlow::WaitUntil(deadline);
        match event {
            Event::UserEvent(UserEvent::TrayAction(value))=>match value["action"].as_str().unwrap_or("") {
                "ready"=>popup.ready(value["height"].as_f64().unwrap()),
                "shown"=>{
                    popup.present(value["generation"].as_u64().unwrap(),value["height"].as_f64().unwrap());
                    popup.validation_script(&format!("(()=>{{const button=document.querySelectorAll('.tray-row,.switch')[{}];const rect=button.getBoundingClientRect();const menu=getComputedStyle(document.querySelector('.tray-menu'));window.ipc.postMessage(JSON.stringify({{action:'testTarget',x:(rect.x+rect.width/2)*devicePixelRatio,y:(rect.y+rect.height/2)*devicePixelRatio,top:menu.borderTopWidth,bottom:menu.borderBottomWidth}}));}})();",clicked.len()));
                },
                "testTarget"=>{
                    assert_eq!(value["top"],"1px");assert_eq!(value["bottom"],"1px");measured_border=true;
                    let root=popup.window.hwnd() as _;
                    let mut point=POINT{x:value["x"].as_f64().unwrap() as i32,y:value["y"].as_f64().unwrap() as i32};
                    unsafe{ClientToScreen(root,&mut point)};
                    let child=unsafe{WindowFromPoint(point)};
                    assert!(child!=root && unsafe{IsChild(root,child)}!=0,"只向本测试菜单的真实子窗口投递鼠标消息");
                    unsafe{
                        SetFocus(child);
                        // Deliver the parent focus notifications that occur while focus moves into WebView2.
                        SendMessageW(root,WM_SETFOCUS,0,0);
                        SendMessageW(root,WM_KILLFOCUS,child as usize,0);
                    };
                    assert!(!popup.lost_external_focus(),"嵌入视图获得焦点被误判为外部失焦");
                    internal_focus_preserved=true;
                    unsafe{ScreenToClient(child,&mut point)};
                    let coordinates=((point.y as u32 & 0xffff)<<16)|(point.x as u32 & 0xffff);
                    unsafe{
                        PostMessageW(child,WM_MOUSEMOVE,0,coordinates as isize);
                        PostMessageW(child,WM_LBUTTONDOWN,1,coordinates as isize);
                        PostMessageW(child,WM_LBUTTONUP,0,coordinates as isize);
                    }
                },
                action if expected.contains(&action)=>{
                    assert_eq!(action,expected[clicked.len()]);
                    clicked.push(action.to_string());popup.hide();
                    if clicked.len()==expected.len(){*control=ControlFlow::Exit;}
                    else{popup.show(600.,600.,json!({"dark":true,"bridge":false,"active":true}));}
                },
                _=>{}
            },
            Event::WindowEvent{window_id,event:WindowEvent::Focused(false),..} if window_id==popup.window.id()=>{
                assert!(!popup.lost_external_focus(),"父窗口失焦提前关闭内部点击");
                internal_focus_events += 1;
            },
            Event::NewEvents(StartCause::ResumeTimeReached{..}) if Instant::now()>=deadline=>{popup.hide();*control=ControlFlow::Exit;},
            _=>{}
        }
    });
    assert!(
        clicked.len() == expected.len()
            && internal_focus_preserved
            && measured_border
            && internal_focus_events > 0,
        "真实鼠标消息没有到达全部菜单动作回执或未覆盖父窗口失焦"
    );
    println!(
        "native menu actions verified: {:?}; internal focus events: {}",
        clicked, internal_focus_events
    );
}
