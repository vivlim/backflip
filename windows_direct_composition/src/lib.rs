mod traits;
use std::marker::PhantomData;

pub use traits::*;

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
// I smashed together
// https://github.com/riley-x/TransparentWindow/blob/master/TransparentWindow/main.cpp
// and
// https://github.com/microsoft/windows-rs/blob/1b4833c504738ac312103fc30024924bbc406fc1/crates/samples/windows/direct2d/src/main.rs

// pub fn show() -> Result<()> {
//     unsafe {
//         CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
//     }
//     let mut window = Overlay<::new()?;
//     window.run()
// }

pub struct OverlayHost<TOverlay, TOverlayArgs>
where
    TOverlayArgs: Sized,
    TOverlay: Direct2DOverlay<TOverlayArgs>,
{
    handle: HWND,
    factory: ID2D1Factory1,
    dxfactory: IDXGIFactory2,

    target: Option<ID2D1DeviceContext>,
    swapchain: Option<IDXGISwapChain1>,
    dcompvisual: Option<IDCompositionVisual>,
    dcomptarget: Option<IDCompositionTarget>,
    // dpi: f32,
    visible: bool,
    occlusion: u32,
    overlay: TOverlay,
    _args: PhantomData<TOverlayArgs>,
}

impl<TOverlay, TOverlayArgs> OverlayHost<TOverlay, TOverlayArgs>
where
    TOverlayArgs: Sized,
    TOverlay: Direct2DOverlay<TOverlayArgs>,
{
    pub fn new(args: TOverlayArgs) -> Result<Self> {
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
        }

        let factory = create_factory()?;
        let dxfactory: IDXGIFactory2 = unsafe { CreateDXGIFactory1()? };

        // duplicated, not sure whether this should go in here or outside
        // let mut dpi = 0.0;
        // let mut dpiy = 0.0;
        // unsafe { factory.GetDesktopDpi(&mut dpi, &mut dpiy) };

        let overlay = TOverlay::new(args, &factory)?;

        Ok(OverlayHost {
            handle: HWND(0),
            factory,
            dxfactory,
            target: None,
            swapchain: None,
            dcompvisual: None,
            dcomptarget: None,
            // dpi,
            visible: false,
            occlusion: 0,
            overlay,
            _args: PhantomData::<TOverlayArgs> {},
        })
    }

    fn render(&mut self) -> Result<()> {
        if self.target.is_none() {
            let device = create_device()?;
            if cfg!(debug_assertions) {
                let mut debug: Option<ID3D12Debug1> = None;
                // Start "DebugView" to listen errors
                // https://docs.microsoft.com/en-us/sysinternals/downloads/debugview
                unsafe { D3D12GetDebugInterface::<ID3D12Debug1>(&mut debug) }
                    .expect("Unable to create debug layer");

                if let Some(debug) = debug {
                    unsafe {
                        debug.EnableDebugLayer();
                        debug.SetEnableGPUBasedValidation(true);
                        debug.SetEnableSynchronizedCommandQueueValidation(true);
                    }
                }
            }
            let swapchain = create_swapchain(&device, self.handle)?;

            let target = create_device_context(&self.factory, &device)?;
            // todo: is it safe to remove all refs to dpi from here?
            // unsafe { target.SetDpi(self.dpi, self.dpi) };

            create_swapchain_bitmap(&swapchain, &target)?;

            let factory = get_dxgi_factory(&device)?;

            let comp = create_dcomposition_device(&device, self.handle)?;
            let visual = unsafe { comp.CreateVisual() }?;
            let comp_target = unsafe { comp.CreateTargetForHwnd(self.handle, true) }?;
            unsafe { visual.SetContent(&swapchain) }?;
            unsafe { comp_target.SetRoot(&visual)? };
            unsafe { comp.Commit() }?;

            self.dcompvisual = Some(visual);
            self.dcomptarget = Some(comp_target);
            self.overlay.create_resources(&target, &device)?;
            self.target = Some(target);
            self.swapchain = Some(swapchain);
            self.create_device_size_resources()?;
        }

        let target = self.target.as_ref().unwrap();
        unsafe { target.BeginDraw() };
        self.overlay.draw(target)?;
        unsafe {
            target.EndDraw(None, None)?;
        }

        if let Err(error) = self.present(1, 0) {
            if error.code() == DXGI_STATUS_OCCLUDED {
                self.occlusion = unsafe {
                    self.dxfactory
                        .RegisterOcclusionStatusWindow(self.handle, WM_USER)?
                };
                self.visible = false;
            } else {
                self.release_device();
            }
        }

        Ok(())
    }

    fn release_device(&mut self) {
        self.target = None;
        self.swapchain = None;
        self.overlay.release_resources();
        self.release_device_resources();
    }

    fn release_device_resources(&mut self) {
        self.overlay.release_sized_resources();
    }

    fn present(&self, sync: u32, flags: u32) -> Result<()> {
        unsafe { self.swapchain.as_ref().unwrap().Present(sync, flags).ok() }
    }

    fn create_device_size_resources(&mut self) -> Result<()> {
        let target = self.target.as_ref().unwrap();
        self.overlay.create_sized_resources(target);

        Ok(())
    }

    fn resize_swapchain_bitmap(&mut self) -> Result<()> {
        if let Some(target) = &self.target {
            let swapchain = self.swapchain.as_ref().unwrap();
            unsafe { target.SetTarget(None) };

            if unsafe {
                swapchain
                    .ResizeBuffers(0, 0, 0, DXGI_FORMAT_UNKNOWN, 0)
                    .is_ok()
            } {
                create_swapchain_bitmap(swapchain, target)?;
                self.create_device_size_resources()?;
            } else {
                self.release_device();
            }

            self.render()?;
        }

        Ok(())
    }

    fn message_handler(&mut self, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        unsafe {
            match message {
                WM_PAINT => {
                    let mut ps = PAINTSTRUCT::default();
                    BeginPaint(self.handle, &mut ps);
                    self.render().unwrap();
                    _ = EndPaint(self.handle, &ps);
                    LRESULT(0)
                }
                WM_SIZE => {
                    if wparam.0 != SIZE_MINIMIZED as usize {
                        self.resize_swapchain_bitmap().unwrap();
                    }
                    LRESULT(0)
                }
                WM_DISPLAYCHANGE => {
                    self.render().unwrap();
                    LRESULT(0)
                }
                WM_USER => {
                    if self.present(0, DXGI_PRESENT_TEST).is_ok() {
                        self.dxfactory.UnregisterOcclusionStatus(self.occlusion);
                        self.occlusion = 0;
                        self.visible = true;
                    }
                    LRESULT(0)
                }
                WM_ACTIVATE => {
                    self.visible = true; // TODO: unpack !HIWORD(wparam);
                    LRESULT(0)
                }
                WM_DESTROY => {
                    PostQuitMessage(0);
                    LRESULT(0)
                }
                _ => DefWindowProcA(self.handle, message, wparam, lparam),
            }
        }
    }

    pub fn run(&mut self) -> Result<()> {
        unsafe {
            let instance = GetModuleHandleA(None)?;
            debug_assert!(instance.0 != 0);
            let window_class = s!("window");

            let wc = WNDCLASSA {
                hCursor: LoadCursorW(None, IDC_HAND)?,
                hInstance: instance.into(),
                lpszClassName: window_class,

                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(Self::wndproc),
                ..Default::default()
            };

            let atom = RegisterClassA(&wc);
            debug_assert!(atom != 0);

            let handle = CreateWindowExA(
                WS_EX_TRANSPARENT | WS_EX_LAYERED | WS_EX_TOPMOST,
                //WS_EX_NOREDIRECTIONBITMAP,
                window_class,
                s!("Sample Window"),
                //WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                //WS_EX_TRANSPARENT | WS_EX_LAYERED | WS_EX_TOPMOST,
                WS_VISIBLE | WS_POPUP,
                300,
                300,
                300,
                300,
                // CW_USEDEFAULT,
                // CW_USEDEFAULT,
                // CW_USEDEFAULT,
                // CW_USEDEFAULT,
                None,
                None,
                instance,
                Some(self as *mut _ as _),
            );

            debug_assert!(handle.0 != 0);
            debug_assert!(handle == self.handle);
            let mut message = MSG::default();

            loop {
                if self.visible {
                    match self.render() {
                        Ok(_) => (),
                        Err(e) => {
                            println!("Fatal error: {:?}", e);
                            return Err(e);
                        }
                    }

                    while PeekMessageA(&mut message, None, 0, 0, PM_REMOVE).into() {
                        if message.message == WM_QUIT {
                            return Ok(());
                        }
                        DispatchMessageA(&message);
                    }
                } else {
                    _ = GetMessageA(&mut message, None, 0, 0);

                    if message.message == WM_QUIT {
                        return Ok(());
                    }

                    DispatchMessageA(&message);
                }
            }
        }
    }

    extern "system" fn wndproc(
        window: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        unsafe {
            if message == WM_NCCREATE {
                let cs = lparam.0 as *const CREATESTRUCTA;
                let this = (*cs).lpCreateParams as *mut Self;
                (*this).handle = window;

                SetWindowLongPtrA(window, GWLP_USERDATA, this as _);
            } else {
                let this = GetWindowLongPtrA(window, GWLP_USERDATA) as *mut Self;

                if !this.is_null() {
                    return (*this).message_handler(message, wparam, lparam);
                }
            }

            DefWindowProcA(window, message, wparam, lparam)
        }
    }
}

fn create_factory() -> Result<ID2D1Factory1> {
    let mut options = D2D1_FACTORY_OPTIONS::default();

    if cfg!(debug_assertions) {
        options.debugLevel = D2D1_DEBUG_LEVEL_INFORMATION;
    }

    unsafe { D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, Some(&options)) }
}

fn create_device_with_type(drive_type: D3D_DRIVER_TYPE) -> Result<ID3D11Device> {
    let mut flags = D3D11_CREATE_DEVICE_BGRA_SUPPORT;

    if cfg!(debug_assertions) {
        flags |= D3D11_CREATE_DEVICE_DEBUG;
    }

    let mut device = None;

    unsafe {
        D3D11CreateDevice(
            None,
            drive_type,
            None,
            flags,
            None,
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            None,
        )
        .map(|()| device.unwrap())
    }
}

fn create_device() -> Result<ID3D11Device> {
    let mut result = create_device_with_type(D3D_DRIVER_TYPE_HARDWARE);

    if let Err(err) = &result {
        if err.code() == DXGI_ERROR_UNSUPPORTED {
            result = create_device_with_type(D3D_DRIVER_TYPE_WARP);
        }
    }

    result
}

fn create_device_context(
    factory: &ID2D1Factory1,
    device: &ID3D11Device,
) -> Result<ID2D1DeviceContext> {
    unsafe {
        let d2device = factory.CreateDevice(&device.cast::<IDXGIDevice>()?)?;

        let target = d2device.CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE)?;

        target.SetUnitMode(D2D1_UNIT_MODE_DIPS);

        Ok(target)
    }
}

fn get_dxgi_factory(device: &ID3D11Device) -> Result<IDXGIFactory2> {
    let dxdevice = device.cast::<IDXGIDevice>()?;
    unsafe { dxdevice.GetAdapter()?.GetParent() }
}

fn create_swapchain_bitmap(swapchain: &IDXGISwapChain1, target: &ID2D1DeviceContext) -> Result<()> {
    let surface: IDXGISurface = unsafe { swapchain.GetBuffer(0)? };

    let props = D2D1_BITMAP_PROPERTIES1 {
        pixelFormat: D2D1_PIXEL_FORMAT {
            format: DXGI_FORMAT_B8G8R8A8_UNORM,
            alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
        },
        dpiX: 96.0,
        dpiY: 96.0,
        bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
        ..Default::default()
    };

    unsafe {
        let bitmap = target.CreateBitmapFromDxgiSurface(&surface, Some(&props))?;
        target.SetTarget(&bitmap);
    };

    Ok(())
}

fn create_swapchain(device: &ID3D11Device, window: HWND) -> Result<IDXGISwapChain1> {
    let factory = get_dxgi_factory(device)?;
    let dxdevice = device.cast::<IDXGIDevice>()?;

    let props = DXGI_SWAP_CHAIN_DESC1 {
        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
        BufferCount: 2,
        SwapEffect: DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
        AlphaMode: DXGI_ALPHA_MODE_PREMULTIPLIED,
        Width: 300,
        Height: 300,
        ..Default::default()
    };

    unsafe { factory.CreateSwapChainForComposition(&dxdevice, &props, None) }
}

fn create_dcomposition_device(device: &ID3D11Device, window: HWND) -> Result<IDCompositionDevice> {
    let dxgidevice = device.cast::<IDXGIDevice>()?;
    let dcompdevice =
        unsafe { DCompositionCreateDevice::<&IDXGIDevice, IDCompositionDevice>(&dxgidevice) }?;
    unsafe { dcompdevice.CreateTargetForHwnd(window, true) };
    Ok(dcompdevice)
}
