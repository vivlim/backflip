mod dxgi;
mod keyhook;

use keyhook::HookMessage;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{mpsc, Arc, RwLock};
use std::thread;
use windows::Win32::UI::WindowsAndMessaging::{DispatchMessageA, GetMessageA, MSG};

//#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release
use global_hotkey::{
    hotkey::{Code, HotKey, Modifiers},
    GlobalHotKeyEvent, GlobalHotKeyManager,
};
use windows::{
    core::IUnknown,
    Win32::{
        Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM},
        UI::WindowsAndMessaging::{self, WH_KEYBOARD_LL},
    },
};

#[cfg(not(target_os = "linux"))]
use std::{cell::RefCell, rc::Rc};
use std::{ptr::null_mut, time::Duration};

use eframe::egui;
use tray_icon::TrayIconBuilder;

fn main() -> Result<(), eframe::Error> {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/icon.png");
    let icon = load_icon(std::path::Path::new(path));

    let (sender, receiver) = mpsc::channel::<HookMessage>();
    {
        let mut ks = keyhook::KEY_SENDER.write().unwrap();
        ks.insert(sender);
    }

    let t = thread::spawn(move || loop {
        match receiver.recv() {
            Ok(d) => println!("received: {:?}", d),
            Err(_) => println!("goodbye"),
        }
    });

    let thread = keyhook::start_thread();

    // let manager = GlobalHotKeyManager::new().unwrap();

    // // construct the hotkey
    // let hotkey = HotKey::new(Some(Modifiers::SUPER), Code::KeyC);

    // // register it
    // manager.register(hotkey);
    // let receiver = GlobalHotKeyEvent::receiver();
    // std::thread::spawn(|| loop {
    //     if let Ok(event) = receiver.try_recv() {
    //         println!("hotkey event: {event:?}");
    //     }
    //     std::thread::sleep(Duration::from_millis(100));
    // });

    // Since egui uses winit under the hood and doesn't use gtk on Linux, and we need gtk for
    // the tray icon to show up, we need to spawn a thread
    // where we initialize gtk and create the tray_icon
    #[cfg(target_os = "linux")]
    std::thread::spawn(|| {
        use tray_icon::menu::Menu;

        gtk::init().unwrap();
        let _tray_icon = TrayIconBuilder::new()
            .with_menu(Box::new(Menu::new()))
            .with_icon(icon)
            .build()
            .unwrap();

        gtk::main();
    });

    // #[cfg(not(target_os = "linux"))]
    // let mut _tray_icon = Rc::new(RefCell::new(None));
    // #[cfg(not(target_os = "linux"))]
    // let tray_c = _tray_icon.clone();

    dxgi::show();

    // eframe::run_native(
    //     "My egui App",
    //     eframe::NativeOptions::default(),
    //     Box::new(move |_cc| {
    //         #[cfg(not(target_os = "linux"))]
    //         {
    //             tray_c
    //                 .borrow_mut()
    //                 .replace(TrayIconBuilder::new().with_icon(icon).build().unwrap());
    //         }
    //         Box::<MyApp>::default()
    //     }),
    // )?;

    Ok(())
}

struct MyApp {
    name: String,
    age: u32,
}

impl Default for MyApp {
    fn default() -> Self {
        Self {
            name: "Arthur".to_owned(),
            age: 42,
        }
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        use tray_icon::TrayIconEvent;

        if let Ok(event) = TrayIconEvent::receiver().try_recv() {
            println!("tray event: {event:?}");
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("My egui Application");
            ui.horizontal(|ui| {
                let name_label = ui.label("Your name: ");
                ui.text_edit_singleline(&mut self.name)
                    .labelled_by(name_label.id);
            });
            ui.add(egui::Slider::new(&mut self.age, 0..=120).text("age"));
            if ui.button("Click each year").clicked() {
                self.age += 1;
            }
            ui.label(format!("Hello '{}', age {}", self.name, self.age));
        });
    }
}

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
