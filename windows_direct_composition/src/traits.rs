use windows::Win32::Graphics::Direct2D::ID2D1DeviceContext;
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

use crate::OverlayHost;

pub trait Direct2DOverlay<TArgs>
where
    Self: Sized,
    TArgs: Sized,
{
    fn new(args: TArgs, d2dfactory: &ID2D1Factory1) -> Result<Self>;
    fn create_resources(
        &mut self,
        target: &ID2D1DeviceContext,
        device: &ID3D11Device,
    ) -> Result<()>;
    fn release_resources(&mut self) -> Result<()>;
    fn create_sized_resources(&mut self, target: &ID2D1DeviceContext) -> Result<()>;
    fn release_sized_resources(&mut self) -> Result<()>;
    fn draw(&self, target: &ID2D1DeviceContext) -> Result<()>;
}
