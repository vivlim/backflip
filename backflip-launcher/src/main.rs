use global_hotkey::GlobalHotKeyEventReceiver;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{mpsc, Arc, RwLock};
use std::thread;
use winit::event_loop::{self, ControlFlow, EventLoopBuilder};

//#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release
use global_hotkey::{
    hotkey::{Code, HotKey, Modifiers},
    GlobalHotKeyEvent, GlobalHotKeyManager,
};

#[cfg(not(target_os = "linux"))]
use std::{cell::RefCell, rc::Rc};
use std::{ptr::null_mut, time::Duration};

use tray_icon::{TrayIconBuilder, TrayIconEvent};

fn main() -> anyhow::Result<()> {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/icon.png");
    let icon = load_icon(std::path::Path::new(path));

    let manager = GlobalHotKeyManager::new().unwrap();

    // construct the hotkey
    let hotkey = HotKey::new(Some(Modifiers::SUPER), Code::Backslash);

    // register it
    manager.register(hotkey);
    let event_loop = EventLoopBuilder::new().build().unwrap();

    // Since egui uses winit under the hood and doesn't use gtk on Linux, and we need gtk for
    // the tray icon to show up, we need to spawn a thread
    // where we initialize gtk and create the tray_icon
    #[cfg(target_os = "linux")]
    std::thread::spawn(|| {
        use tray_icon::menu::Menu;

        gtk::init().unwrap();
        let tray_icon = TrayIconBuilder::new()
            .with_menu(Box::new(Menu::new()))
            .with_icon(icon)
            .build()
            .unwrap();

        gtk::main();
    });

    #[cfg(not(target_os = "linux"))]
    let tray_icon = TrayIconBuilder::new()
        .with_icon(icon)
        .with_tooltip("tray")
        .build()
        .unwrap();

    let hotkey_channel = GlobalHotKeyEvent::receiver();
    let tray_channel = TrayIconEvent::receiver();

    event_loop.run(move |_event, event_loop| {
        event_loop.set_control_flow(ControlFlow::Poll);

        if let Ok(event) = tray_channel.try_recv() {
            println!("tray {event:?}");
        }

        if let Ok(event) = hotkey_channel.try_recv() {
            println!("hotkey {event:?}");
        }
    });

    Ok(())
}

struct HotkeyPress {}

fn load_icon(path: &std::path::Path) -> tray_icon::Icon {
    let (icon_rgba, icon_width, icon_height) = {
        let image = image::open(path)
            .expect("Failed to open icon path")
            .into_rgba8();
        let (width, height) = image.dimensions();
        let rgba = image.into_raw();
        (rgba, width, height)
    };
    tray_icon::Icon::from_rgba(icon_rgba, icon_width, icon_height).expect("Failed to open icon")
}
