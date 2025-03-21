using Direct2D;
using DXGI;
using System;
using System.Collections.Generic;
using System.Linq;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading.Tasks;
using WIC;
using GlobalStructures;
using static GlobalStructures.GlobalTools;
using BlitzWinRT;

namespace MarkdownTest;

internal static class D2DContext
{
    static ID2D1Factory m_pD2DFactory = null;
    static ID2D1Factory1 m_pD2DFactory1 = null;
    static IWICImagingFactory m_pWICImagingFactory = null;

    static IntPtr m_pD3D11DevicePtr = IntPtr.Zero; //Used in CreateSwapChain
    static ID3D11DeviceContext m_pD3D11DeviceContext = null; // Released in Clean : not used
    static IDXGIDevice1 m_pDXGIDevice = null; // Released in Clean

    static ID2D1Device m_pD2DDevice = null; // Released in CreateDeviceContext
    static ID2D1DeviceContext m_pD2DDeviceContext = null; // Released in Clean
    static ID2D1DeviceContext3 m_pD2DDeviceContext3 = null;

    static ID2D1Bitmap m_pD2DBitmapBackground = null;
    static ID2D1Bitmap m_pD2DBitmap = null;
    static ID2D1Bitmap m_pD2DBitmap1 = null;

    static ID2D1Bitmap1 m_pD2DTargetBitmap = null;
    static IDXGISwapChain1 m_pDXGISwapChain1 = null;
    static ID2D1SolidColorBrush m_pMainBrush = null;


    static void CleanDeviceResources()
    {
        SafeRelease(ref m_pD2DBitmap);
        SafeRelease(ref m_pD2DBitmap1);
        SafeRelease(ref m_pD2DBitmapBackground);
        SafeRelease(ref m_pMainBrush);
    }

    static void Clean()
    {
        SafeRelease(ref m_pD2DDeviceContext);
        SafeRelease(ref m_pD2DDeviceContext3);

        CleanDeviceResources();

        SafeRelease(ref m_pD2DTargetBitmap);
        SafeRelease(ref m_pDXGISwapChain1);

        SafeRelease(ref m_pD3D11DeviceContext);
        if (m_pD3D11DevicePtr != IntPtr.Zero)
            Marshal.Release(m_pD3D11DevicePtr);
        SafeRelease(ref m_pDXGIDevice);

        SafeRelease(ref m_pWICImagingFactory);
        SafeRelease(ref m_pD2DFactory1);
        SafeRelease(ref m_pD2DFactory);
    }
}
