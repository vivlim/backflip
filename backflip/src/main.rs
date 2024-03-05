use arboard::Clipboard;
use crossbeam_channel::Receiver;
use global_hotkey::{GlobalHotKeyEventReceiver, HotKeyState};
use std::sync::{mpsc, Arc, RwLock};
use std::thread::{self, JoinHandle};

//#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release
use global_hotkey::{
    hotkey::{Code, HotKey, Modifiers},
    GlobalHotKeyEvent, GlobalHotKeyManager,
};

#[cfg(not(target_os = "linux"))]
use std::{cell::RefCell, rc::Rc};
use std::{ptr::null_mut, time::Duration};

use eframe::egui::{self, Context, Key, ViewportCommand, ViewportId};
use tray_icon::{TrayIconBuilder, TrayIconEvent, TrayIconEventReceiver};

fn main() -> Result<(), eframe::Error> {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/icon.png");
    let icon = load_icon(std::path::Path::new(path));

    let manager = GlobalHotKeyManager::new().unwrap();

    // construct the hotkey
    let hotkey = HotKey::new(Some(Modifiers::SUPER), Code::Backslash);

    // register it
    manager.register(hotkey);

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

    #[cfg(not(target_os = "linux"))]
    let mut _tray_icon = Rc::new(RefCell::new(None));
    #[cfg(not(target_os = "linux"))]
    let tray_c = _tray_icon.clone();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([320.0, 240.0]),

        ..Default::default()
    };

    let app = MyApp {
        name: "aa".to_string(),
        age: 69,
        hotkey_receiver: GlobalHotKeyEvent::receiver().clone(),
        tray_receiver: TrayIconEvent::receiver().clone(),
        sessions: vec![],
        wakeup_thread: None,
        wakeup_requests: None,
    };

    eframe::run_native(
        "My egui App",
        options,
        Box::new(move |_cc| {
            #[cfg(not(target_os = "linux"))]
            {
                tray_c
                    .borrow_mut()
                    .replace(TrayIconBuilder::new().with_icon(icon).build().unwrap());
            }
            Box::new(app)
        }),
    )?;

    Ok(())
}

struct HotkeyPress {}

struct MyApp {
    name: String,
    age: u32,
    hotkey_receiver: GlobalHotKeyEventReceiver,
    tray_receiver: TrayIconEventReceiver,
    sessions: Vec<Option<Session>>,
    wakeup_thread: Option<JoinHandle<()>>,
    wakeup_requests: Option<Receiver<HotkeyPress>>,
}

struct Session {
    captured_clipboard: String,
    viewport_id: ViewportId,
    title: String,
    requested_focus: bool,
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        use tray_icon::TrayIconEvent;

        if let None = self.wakeup_thread {
            let hotkey_receiver = self.hotkey_receiver.clone();
            let (wakeup_sender, wakeup_receiver) = crossbeam_channel::unbounded();
            self.wakeup_requests = Some(wakeup_receiver);
            let ctx = ctx.clone();
            self.wakeup_thread = Some(std::thread::spawn(move || loop {
                if let Ok(GlobalHotKeyEvent {
                    state: HotKeyState::Pressed,
                    ..
                }) = hotkey_receiver.recv()
                {
                    wakeup_sender.send(HotkeyPress {});
                    ctx.request_repaint();
                }
                std::thread::sleep(Duration::from_millis(100));
            }));
        }

        if let Ok(event) = self.tray_receiver.try_recv() {
            println!("tray event: {event:?}");
        }

        if let Some(recv) = &self.wakeup_requests {
            if let Ok(event) = recv.try_recv() {
                self.new_session();
            }
        }

        for session in &mut self.sessions {
            let mut closing = false;
            if let Some(session) = session {
                ctx.show_viewport_immediate(
                    session.viewport_id,
                    egui::ViewportBuilder::default()
                        .with_title(&session.title)
                        .with_inner_size([500.0, 200.0]),
                    |ctx, class| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            ui.text_edit_multiline(&mut session.captured_clipboard);
                            ui.label("S: serialize json. X: deserialize json");
                            ui.label("D: reverse slashes");
                            ui.label("enter: copy. escape: close.");
                        });

                        if ctx.input(|i| i.viewport().close_requested()) {
                            closing = true;
                        }

                        if ctx.input(|i| i.key_released(Key::Escape)) {
                            closing = true;
                        }
                        if ctx.input(|i| i.key_released(Key::Enter)) {
                            let mut clipboard = Clipboard::new().unwrap();
                            clipboard.set_text(&session.captured_clipboard).unwrap();
                            closing = true;
                        }

                        if ctx.input(|i| i.key_released(Key::D)) {
                            session.captured_clipboard = session
                                .captured_clipboard
                                .replace("\\", "THISWASABACKSLASH");
                            session.captured_clipboard =
                                session.captured_clipboard.replace("/", "\\");
                            session.captured_clipboard =
                                session.captured_clipboard.replace("THISWASABACKSLASH", "/");
                        }

                        if ctx.input(|i| i.key_released(Key::S)) {
                            session.captured_clipboard =
                                serde_json::to_string(&session.captured_clipboard).unwrap();
                        }

                        if ctx.input(|i| i.key_released(Key::A)) {
                            match serde_json::from_str(&session.captured_clipboard) {
                                Ok(s) => session.captured_clipboard = s,
                                Err(_) => (),
                            }
                        }
                    },
                );
                if session.requested_focus {
                    session.requested_focus = false;
                    ctx.send_viewport_cmd_to(session.viewport_id, ViewportCommand::Focus);
                }
            }
            if closing {
                // Remove the session
                _ = session.take();
            }
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

impl MyApp {
    pub fn new_session(&mut self) -> anyhow::Result<()> {
        let mut captured_clipboard: Option<String> = None;

        #[cfg(target_os = "windows")]
        {
            use clipboard_win::{formats, get_clipboard, set_clipboard};
            captured_clipboard = match clipboard_win::get_clipboard_string() {
                Ok(s) => Some(s),
                Err(_) => match clipboard_win::get_clipboard(formats::FileList) {
                    Ok(files) => Some(files.join(",")),
                    Err(_) => None,
                },
            };
        }
        #[cfg(not(target_os = "windows"))]
        {
            let mut clipboard = Clipboard::new().unwrap();
            captured_clipboard = clipboard.get_text()?;
        }

        if let Some(c) = captured_clipboard {
            let s = Session {
                captured_clipboard: c,
                viewport_id: ViewportId::from_hash_of(format!("session-{}", self.sessions.len())),
                title: format!("backflip {}", self.sessions.len()),
                requested_focus: true,
            };
            self.sessions.push(Some(s));
        }
        Ok(())
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
