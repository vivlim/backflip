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

fn main() {
    let mut host = OverlayHost::<ClockOverlay, ()>::new(()).unwrap();
    host.run().unwrap();
}

pub struct ClockOverlay {
    angles: Angles,
    clock: Option<ID2D1Bitmap1>,
    shadow: Option<ID2D1Effect>,
    brush: Option<ID2D1SolidColorBrush>,
    dpi: f32,

    style: ID2D1StrokeStyle,
    manager: IUIAnimationManager,
    variable: IUIAnimationVariable,
    frequency: i64,
}

#[derive(Default)]
struct Angles {
    second: f32,
    minute: f32,
    hour: f32,
}

impl Angles {
    fn now() -> Self {
        let time = unsafe { GetLocalTime() };

        let second = (time.wSecond as f32 + time.wMilliseconds as f32 / 1000.0) * 6.0;
        let minute = time.wMinute as f32 * 6.0 + second / 60.0;
        let hour = (time.wHour % 12) as f32 * 30.0 + minute / 12.0;

        Self {
            second,
            minute,
            hour,
        }
    }
}

impl ClockOverlay {
    fn create_clock(&self, target: &ID2D1DeviceContext) -> Result<ID2D1Bitmap1> {
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

    fn draw_clock(&self, target: &ID2D1DeviceContext) -> Result<()> {
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

        let swing = unsafe {
            target.DrawEllipse(&ellipse, brush, radius / 20.0, None);
            self.variable.GetValue()?
        };
        let mut angles = Angles::now();

        if swing < 1.0 {
            if self.angles.second > angles.second {
                angles.second += 360.0;
            }
            if self.angles.minute > angles.minute {
                angles.minute += 360.0;
            }
            if self.angles.hour > angles.hour {
                angles.hour += 360.0;
            }

            angles.second *= swing as f32;
            angles.minute *= swing as f32;
            angles.hour *= swing as f32;
        }

        unsafe {
            target.SetTransform(&(Matrix3x2::rotation(angles.second, 0.0, 0.0) * translation));

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

            target.SetTransform(&(Matrix3x2::rotation(angles.minute, 0.0, 0.0) * translation));

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

            target.SetTransform(&(Matrix3x2::rotation(angles.hour, 0.0, 0.0) * translation));

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

impl Direct2DOverlay<()> for ClockOverlay {
    fn new(args: (), d2dfactory: &ID2D1Factory1) -> Result<Self> {
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

        Ok(ClockOverlay {
            style,
            manager,
            dpi,
            frequency,
            variable,
            angles: Angles::now(),
            clock: None,
            shadow: None,
            brush: None,
        })
    }

    fn create_resources(
        &mut self,
        target: &ID2D1DeviceContext,
        device: &ID3D11Device,
    ) -> Result<()> {
        self.brush = create_brush(&target).ok();
        Ok(())
    }

    fn create_sized_resources(&mut self, target: &ID2D1DeviceContext) -> Result<()> {
        let clock = self.create_clock(target)?;
        self.shadow = create_shadow(target, &clock).ok();
        self.clock = Some(clock);

        Ok(())
    }

    fn release_resources(&mut self) -> Result<()> {
        self.brush = None; // difference, not in release_sizef
        Ok(())
    }

    fn release_sized_resources(&mut self) -> Result<()> {
        self.clock = None;
        self.shadow = None;
        Ok(())
    }

    fn draw(&self, target: &ID2D1DeviceContext) -> Result<()> {
        let clock = self.clock.as_ref().unwrap();
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
            target.SetTarget(clock);
            target.Clear(None);
            self.draw_clock(target)?;
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
                clock,
                None,
                None,
                D2D1_INTERPOLATION_MODE_LINEAR,
                D2D1_COMPOSITE_MODE_SOURCE_OVER,
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
