use std::sync::mpsc::{Receiver, Sender};
use std::sync::{mpsc, Arc, RwLock};
use std::thread;
use windows::Win32::Graphics::Direct2D::{
    ID2D1Bitmap1, ID2D1Effect, ID2D1SolidColorBrush, ID2D1StrokeStyle,
};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};
use windows::Win32::System::Performance::QueryPerformanceFrequency;
use windows::Win32::UI::Animation::{
    IUIAnimationManager, IUIAnimationVariable, UIAnimationManager,
};
use windows::Win32::UI::WindowsAndMessaging::{DispatchMessageA, GetMessageA, MSG};
use windows::{
    core::*,
    Foundation::Numerics::*,
    Win32::Foundation::*,
    Win32::Graphics::Direct2D::Common::*,
    Win32::Graphics::Direct2D::*,
    Win32::Graphics::Direct3D::*,
    Win32::Graphics::Direct3D11::*,
    Win32::Graphics::Dxgi::Common::*,
    Win32::Graphics::Dxgi::*,
    Win32::Graphics::{
        Direct3D12::{D3D12GetDebugInterface, ID3D12Debug, ID3D12Debug1},
        DirectComposition::{
            DCompositionCreateDevice, IDCompositionDevice, IDCompositionTarget, IDCompositionVisual,
        },
        Gdi::*,
    },
    Win32::System::Com::*,
    Win32::System::LibraryLoader::*,
    Win32::System::Performance::*,
    Win32::System::SystemInformation::GetLocalTime,
    Win32::UI::Animation::*,
    Win32::UI::WindowsAndMessaging::*,
};
use windows_direct_composition::{Direct2DOverlay, OverlayHost};
use windows_lowlevel_hooks::HookMessage;

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

fn main() -> anyhow::Result<()> {
    // let path = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/icon.png");
    // let icon = load_icon(std::path::Path::new(path));

    let (sender, receiver) = mpsc::channel::<HookMessage>();
    {
        let mut ks = windows_lowlevel_hooks::KEY_SENDER.write().unwrap();
        ks.insert(sender);
    }

    let t = thread::spawn(move || loop {
        match receiver.recv() {
            Ok(d) => println!("received: {:?}", d),
            Err(_) => println!("goodbye"),
        }
    });

    let thread = windows_lowlevel_hooks::start_thread();

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
    let args = BackflipArgs {};
    let mut host = OverlayHost::<BackflipOverlay, BackflipArgs>::new(args)?;
    host.run()?;

    Ok(())
}

pub struct BackflipOverlay {
    bitmap: Option<ID2D1Bitmap1>,
    shadow: Option<ID2D1Effect>,
    brush: Option<ID2D1SolidColorBrush>,
    dpi: f32,

    style: ID2D1StrokeStyle,
    manager: IUIAnimationManager,
    variable: IUIAnimationVariable,
    frequency: i64,
}

pub struct BackflipArgs {}

impl Direct2DOverlay<BackflipArgs> for BackflipOverlay {
    fn new(
        args: BackflipArgs,
        d2dfactory: &windows::Win32::Graphics::Direct2D::ID2D1Factory1,
    ) -> windows::core::Result<Self> {
        let style = create_style(&d2dfactory)?;
        let manager: IUIAnimationManager =
            unsafe { CoCreateInstance(&UIAnimationManager, None, CLSCTX_ALL)? };
        let transition = create_transition()?;

        let mut dpi = 0.0;
        let mut dpiy = 0.0;
        unsafe { d2dfactory.GetDesktopDpi(&mut dpi, &mut dpiy) };

        let mut frequency = 0;
        unsafe { QueryPerformanceFrequency(&mut frequency)? };

        let variable = unsafe {
            let variable = manager.CreateAnimationVariable(0.0)?;

            manager.ScheduleTransition(&variable, &transition, get_time(frequency)?)?;

            variable
        };
        Ok(BackflipOverlay {
            style,
            manager,
            dpi,
            frequency,
            variable,
            bitmap: None,
            shadow: None,
            brush: None,
        })
    }

    fn create_resources(
        &mut self,
        target: &windows::Win32::Graphics::Direct2D::ID2D1DeviceContext,
        device: &windows::Win32::Graphics::Direct3D11::ID3D11Device,
    ) -> windows::core::Result<()> {
        self.brush = create_brush(&target).ok();

        Ok(())
    }

    fn release_resources(&mut self) -> windows::core::Result<()> {
        todo!()
    }

    fn create_sized_resources(
        &mut self,
        target: &windows::Win32::Graphics::Direct2D::ID2D1DeviceContext,
    ) -> windows::core::Result<()> {
        let bitmap = self.create_bitmap(&target)?;
        self.shadow = Some(create_shadow(target, &bitmap)?);
        self.bitmap = Some(bitmap);
        Ok(())
    }

    fn release_sized_resources(&mut self) -> windows::core::Result<()> {
        self.bitmap = None;
        self.shadow = None;
        Ok(())
    }

    fn draw(
        &self,
        target: &windows::Win32::Graphics::Direct2D::ID2D1DeviceContext,
    ) -> windows::core::Result<()> {
        let bitmap = self.bitmap.as_ref().unwrap();
        let shadow = self.shadow.as_ref().unwrap();

        unsafe {
            self.manager.Update(get_time(self.frequency)?, None)?;

            target.Clear(Some(&D2D1_COLOR_F {
                r: 1.0,
                g: 1.0,
                b: 1.0,
                a: 0.0,
            }));

            let previous = target.GetTarget()?;
            target.SetTarget(bitmap);
            target.Clear(None);
            self.draw_bitmap(target)?;
            target.SetTarget(&previous);
            target.SetTransform(&Matrix3x2::translation(5.0, 5.0));

            target.DrawImage(
                &shadow.GetOutput()?,
                None,
                None,
                D2D1_INTERPOLATION_MODE_LINEAR,
                D2D1_COMPOSITE_MODE_SOURCE_OVER,
            );

            target.SetTransform(&Matrix3x2::identity());

            target.DrawImage(
                bitmap,
                None,
                None,
                D2D1_INTERPOLATION_MODE_LINEAR,
                D2D1_COMPOSITE_MODE_SOURCE_OVER,
            );
        }

        Ok(())
    }
}

impl BackflipOverlay {
    fn create_bitmap(&self, target: &ID2D1DeviceContext) -> Result<ID2D1Bitmap1> {
        let size_f = unsafe { target.GetSize() };

        let size_u = D2D_SIZE_U {
            width: (size_f.width * self.dpi / 96.0) as u32,
            height: (size_f.height * self.dpi / 96.0) as u32,
        };

        let properties = D2D1_BITMAP_PROPERTIES1 {
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
            },
            dpiX: self.dpi,
            dpiY: self.dpi,
            bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET,
            ..Default::default()
        };

        unsafe { target.CreateBitmap2(size_u, None, 0, &properties) }
    }

    fn draw_bitmap(&self, target: &ID2D1DeviceContext) -> Result<()> {
        let brush = self.brush.as_ref().unwrap();

        let size = unsafe { target.GetSize() };

        #[allow(clippy::manual_clamp)]
        let radius = size.width.min(size.height).max(200.0) / 2.0 - 50.0;
        let translation = Matrix3x2::translation(size.width / 2.0, size.height / 2.0);
        unsafe { target.SetTransform(&translation) };

        let ellipse = D2D1_ELLIPSE {
            point: D2D_POINT_2F::default(),
            radiusX: radius,
            radiusY: radius,
        };

        unsafe {
            target.SetTransform(&(Matrix3x2::rotation(0.0, 0.0, 0.0) * translation));

            target.DrawLine(
                D2D_POINT_2F::default(),
                D2D_POINT_2F {
                    x: 0.0,
                    y: -(radius * 0.75),
                },
                brush,
                radius / 25.0,
                &self.style,
            );

            target.SetTransform(&(Matrix3x2::rotation(1.0, 0.0, 0.0) * translation));

            target.DrawLine(
                D2D_POINT_2F::default(),
                D2D_POINT_2F {
                    x: 0.0,
                    y: -(radius * 0.75),
                },
                brush,
                radius / 15.0,
                &self.style,
            );

            target.SetTransform(&(Matrix3x2::rotation(2.0, 0.0, 0.0) * translation));

            target.DrawLine(
                D2D_POINT_2F::default(),
                D2D_POINT_2F {
                    x: 0.0,
                    y: -(radius * 0.5),
                },
                brush,
                radius / 10.0,
                &self.style,
            );
        }

        Ok(())
    }
}

fn create_shadow(target: &ID2D1DeviceContext, clock: &ID2D1Bitmap1) -> Result<ID2D1Effect> {
    unsafe {
        let shadow = target.CreateEffect(&CLSID_D2D1Shadow)?;

        shadow.SetInput(0, clock, true);
        Ok(shadow)
    }
}

fn create_brush(target: &ID2D1DeviceContext) -> Result<ID2D1SolidColorBrush> {
    let color = D2D1_COLOR_F {
        r: 0.92,
        g: 0.38,
        b: 0.208,
        a: 0.6,
    };

    let properties = D2D1_BRUSH_PROPERTIES {
        opacity: 0.8,
        transform: Matrix3x2::identity(),
    };

    unsafe { target.CreateSolidColorBrush(&color, Some(&properties)) }
}

fn create_style(factory: &ID2D1Factory1) -> Result<ID2D1StrokeStyle> {
    let props = D2D1_STROKE_STYLE_PROPERTIES {
        startCap: D2D1_CAP_STYLE_ROUND,
        endCap: D2D1_CAP_STYLE_TRIANGLE,
        ..Default::default()
    };

    unsafe { factory.CreateStrokeStyle(&props, None) }
}

fn create_transition() -> Result<IUIAnimationTransition> {
    unsafe {
        let library: IUIAnimationTransitionLibrary =
            CoCreateInstance(&UIAnimationTransitionLibrary, None, CLSCTX_ALL)?;
        library.CreateAccelerateDecelerateTransition(5.0, 1.0, 0.2, 0.8)
    }
}

fn get_time(frequency: i64) -> Result<f64> {
    unsafe {
        let mut time = 0;
        QueryPerformanceCounter(&mut time)?;
        Ok(time as f64 / frequency as f64)
    }
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
