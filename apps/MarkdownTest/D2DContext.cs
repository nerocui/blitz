using Direct2D;
using DXGI;
using System;
using System.Runtime.InteropServices;
using WIC;
using GlobalStructures;
using static GlobalStructures.GlobalTools;
using Microsoft.UI.Xaml;
using Windows.Foundation;
using static DXGI.DXGITools;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;

namespace MarkdownTest;

[ComImport, Guid("63aad0b8-7c24-40ff-85a8-640d944cc325"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
public interface ISwapChainPanelNative
{
    [PreserveSig]
    HRESULT SetSwapChain(IDXGISwapChain swapChain);
}

public static class D2DContext
{
    [DllImport("User32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
    public static extern uint GetDpiForWindow(IntPtr hwnd);

    [DllImport("Kernel32.dll", SetLastError = true, CharSet = CharSet.Auto)]
    public static extern bool QueryPerformanceFrequency(out LARGE_INTEGER lpFrequency);

    [DllImport("Libraries/BlitzWinRT.dll", CharSet = CharSet.Unicode, CallingConvention = CallingConvention.StdCall)]
    internal static extern Int32 DllGetActivationFactory(IntPtr deviceContextPtr, out IntPtr class_instance);

    static ID2D1Factory m_pD2DFactory = null;
    static ID2D1Factory1 m_pD2DFactory1 = null;
    static IWICImagingFactory m_pWICImagingFactory = null;

    static IntPtr m_pD3D11DevicePtr = IntPtr.Zero; //Used in CreateSwapChain
    static ID3D11DeviceContext m_pD3D11DeviceContext = null; // Released in Clean : not used
    static IDXGIDevice1 m_pDXGIDevice = null; // Released in Clean

    static ID2D1Device m_pD2DDevice = null; // Released in CreateDeviceContext
    static ID2D1DeviceContext m_pD2DDeviceContext = null; // Released in Clean

    static ID2D1Bitmap1 m_pD2DTargetBitmap = null;
    static IDXGISwapChain1 m_pDXGISwapChain1 = null;

    static private bool bRender = true;
    static private ulong nLastTime = 0, nTotalTime = 0;
    static private uint nNbTotalFrames = 0, nLastNbFrames = 0;
    static private IntPtr _hWndMain = IntPtr.Zero;
    static private Microsoft.UI.Windowing.AppWindow _apw;
    static private BlitzWinRT.D2DRenderer _d2drenderer;
    static private LARGE_INTEGER liFreq;

    static private SwapChainPanel _swapChainPanel = null;
    static private string _markdown = null;
    static private bool _rendered = false;

    public static void Initialize(IntPtr hWndMain)
    {
        _hWndMain = hWndMain;
        Microsoft.UI.WindowId myWndId = Microsoft.UI.Win32Interop.GetWindowIdFromWindow(hWndMain);
        _apw = Microsoft.UI.Windowing.AppWindow.GetFromWindowId(myWndId);

        m_pWICImagingFactory = (IWICImagingFactory)Activator.CreateInstance(Type.GetTypeFromCLSID(WICTools.CLSID_WICImagingFactory));

        liFreq = new LARGE_INTEGER();
        QueryPerformanceFrequency(out liFreq);

        HRESULT hr = CreateD2D1Factory();
        if (hr == HRESULT.S_OK)
        {
            hr = CreateDeviceContext();
        }
    }

    public static void scpD2D_SizeChanged(object sender, SizeChangedEventArgs e)
    {
        //string sMessage = "NewSize = " + string.Format("{0}, {1}", e.NewSize.Width, e.NewSize.Height);
        //System.Diagnostics.Debug.WriteLine(sMessage);
        Resize(e.NewSize);
    }

    public static HRESULT CreateD2D1Factory()
    {
        HRESULT hr = HRESULT.S_OK;
        D2D1_FACTORY_OPTIONS options = new D2D1_FACTORY_OPTIONS();

        // Needs "Enable native code Debugging"
#if DEBUG
        options.debugLevel = D2D1_DEBUG_LEVEL.D2D1_DEBUG_LEVEL_INFORMATION;
#endif

        hr = D2DTools.D2D1CreateFactory(D2D1_FACTORY_TYPE.D2D1_FACTORY_TYPE_SINGLE_THREADED, ref D2DTools.CLSID_D2D1Factory, ref options, out m_pD2DFactory);
        m_pD2DFactory1 = (ID2D1Factory1)m_pD2DFactory;
        return hr;
    }

    public static void SetupRendering(SwapChainPanel swapChainPanel, string markdown)
    {
        _swapChainPanel = swapChainPanel;
        _markdown = markdown;
        var hr = CreateSwapChain(IntPtr.Zero);
        if (hr == HRESULT.S_OK)
        {
            hr = ConfigureSwapChain();
            ISwapChainPanelNative panelNative = WinRT.CastExtensions.As<ISwapChainPanelNative>(swapChainPanel);
            hr = panelNative.SetSwapChain(m_pDXGISwapChain1);
        }
        swapChainPanel.SizeChanged += scpD2D_SizeChanged;
        CompositionTarget.Rendering += CompositionTarget_Rendering;

        try
        {
            // Get the ID2D1DeviceContext pointer for use with the Rust component
            IntPtr nativeContext = Marshal.GetComInterfaceForObject(m_pD2DDeviceContext, typeof(ID2D1DeviceContext));
            
            // Convert the pointer to a UInt64 as expected by D2DRenderer constructor
            ulong contextHandle = (ulong)nativeContext.ToInt64();
            
            // Create the D2DRenderer directly through its constructor
            _d2drenderer = new BlitzWinRT.D2DRenderer(contextHandle);
            
            // Make sure to release the COM reference we acquired
            Marshal.Release(nativeContext);
        }
        catch (Exception ex)
        {
            System.Diagnostics.Debug.WriteLine($"Error creating D2DRenderer: {ex.Message}");
            // Fallback - renderer will be null but app won't crash
        }
        
        // Set initial theme based on app theme
        var appTheme = ((FrameworkElement)swapChainPanel).ActualTheme;
        SetTheme(appTheme == ElementTheme.Dark);
    }

    public static HRESULT Render()
    {
        HRESULT hr = HRESULT.S_OK;
        if (m_pD2DDeviceContext != null)
        {
            // Check if _d2drenderer is not null before calling Render
            if (_d2drenderer != null)
            {
                try
                {
                    _d2drenderer.Render(_markdown);
                }
                catch (Exception ex)
                {
                    System.Diagnostics.Debug.WriteLine($"Error calling D2DRenderer.Render: {ex.Message}");
                }
            }
            else
            {
                System.Diagnostics.Debug.WriteLine("D2DRenderer is null. Trying to re-initialize...");
                // Try to reinitialize the renderer
                if (m_pD2DDeviceContext != null)
                {
                    try
                    {
                        IntPtr nativeContext = Marshal.GetComInterfaceForObject(m_pD2DDeviceContext, typeof(ID2D1DeviceContext));
                        ulong contextHandle = (ulong)nativeContext.ToInt64();
                        _d2drenderer = new BlitzWinRT.D2DRenderer(contextHandle);
                        Marshal.Release(nativeContext);
                        
                        // Now try to render again
                        _d2drenderer.Render(_markdown);
                    }
                    catch (Exception ex)
                    {
                        System.Diagnostics.Debug.WriteLine($"Error reinitializing D2DRenderer: {ex.Message}");
                    }
                }
            }

            if ((uint)hr == D2DTools.D2DERR_RECREATE_TARGET)
            {
                m_pD2DDeviceContext.SetTarget(null);
                SafeRelease(ref m_pD2DDeviceContext);
                hr = CreateDeviceContext();
                hr = CreateSwapChain(IntPtr.Zero);
                hr = ConfigureSwapChain();
            }
            hr = m_pDXGISwapChain1.Present(1, 0);
        }
        return (hr);
    }

    private static void CompositionTarget_Rendering(object sender, object e)
    {
        HRESULT hr = HRESULT.S_OK;
        if (bRender && !_rendered)
        {
            Render();
            _rendered = true;
            if (m_pDXGISwapChain1 != null)
            {
                DXGI_FRAME_STATISTICS fs = new DXGI_FRAME_STATISTICS();
                hr = m_pDXGISwapChain1.GetFrameStatistics(out fs);
                // 0x887A000B DXGI_ERROR_FRAME_STATISTICS_DISJOINT            
                if (hr == HRESULT.S_OK)
                {
                    ulong nCurrentTime = (ulong)fs.SyncQPCTime.QuadPart;
                    nNbTotalFrames += fs.PresentCount - nLastNbFrames;
                    if (nLastTime != 0)
                    {
                        nTotalTime += (nCurrentTime - nLastTime);
                        double nSeconds = nTotalTime / (ulong)liFreq.QuadPart;
                        if (nSeconds >= 1)
                        {
                            // TODO: add back later
                            // tbFPS.Text = nNbTotalFrames.ToString() + " FPS";
                            nNbTotalFrames = 0;
                            nTotalTime = 0;
                        }
                    }
                    nLastNbFrames = fs.PresentCount;
                    nLastTime = nCurrentTime;
                }
            }
        }
    }

    public static void UnloadPage()
    {
        _swapChainPanel.SizeChanged -= scpD2D_SizeChanged;
        _swapChainPanel = null;
        _markdown = null;
        _d2drenderer = null;
        _rendered = false;
        SafeRelease(ref m_pDXGISwapChain1);
    }

    public static HRESULT Resize(Size sz)
    {
        HRESULT hr = HRESULT.S_OK;

        if (m_pDXGISwapChain1 != null)
        {
            if (m_pD2DDeviceContext != null)
                m_pD2DDeviceContext.SetTarget(null);

            if (m_pD2DTargetBitmap != null)
                SafeRelease(ref m_pD2DTargetBitmap);

            // 0, 0 => HRESULT: 0x80070057 (E_INVALIDARG) if not CreateSwapChainForHwnd
            //hr = m_pDXGISwapChain1.ResizeBuffers(
            // 2,
            // 0,
            // 0,
            // DXGI_FORMAT.DXGI_FORMAT_B8G8R8A8_UNORM,
            // 0
            // );
            if (sz.Width != 0 && sz.Height != 0)
            {
                hr = m_pDXGISwapChain1.ResizeBuffers(
                  2,
                  (uint)sz.Width,
                  (uint)sz.Height,
                  DXGI_FORMAT.DXGI_FORMAT_B8G8R8A8_UNORM,
                  0
                  );
            }
            ConfigureSwapChain();
        }
        return (hr);
    }

    public static HRESULT CreateDeviceContext()
    {
        HRESULT hr = HRESULT.S_OK;
        uint creationFlags = (uint)D3D11_CREATE_DEVICE_FLAG.D3D11_CREATE_DEVICE_BGRA_SUPPORT;

        // Needs "Enable native code Debugging"
#if DEBUG
        creationFlags |= (uint)D3D11_CREATE_DEVICE_FLAG.D3D11_CREATE_DEVICE_DEBUG;
#endif

        int[] aD3D_FEATURE_LEVEL = new int[] { (int)D3D_FEATURE_LEVEL.D3D_FEATURE_LEVEL_11_1, (int)D3D_FEATURE_LEVEL.D3D_FEATURE_LEVEL_11_0,
                (int)D3D_FEATURE_LEVEL.D3D_FEATURE_LEVEL_10_1, (int)D3D_FEATURE_LEVEL.D3D_FEATURE_LEVEL_10_0, (int)D3D_FEATURE_LEVEL.D3D_FEATURE_LEVEL_9_3,
                (int)D3D_FEATURE_LEVEL.D3D_FEATURE_LEVEL_9_2, (int)D3D_FEATURE_LEVEL.D3D_FEATURE_LEVEL_9_1};

        D3D_FEATURE_LEVEL featureLevel;
        hr = D2DTools.D3D11CreateDevice(null,    // specify null to use the default adapter
            D3D_DRIVER_TYPE.D3D_DRIVER_TYPE_HARDWARE,
            IntPtr.Zero,
            creationFlags,              // optionally set debug and Direct2D compatibility flags
                                        //pD3D_FEATURE_LEVEL,              // list of feature levels this app can support
            aD3D_FEATURE_LEVEL,
            //(uint)Marshal.SizeOf(aD3D_FEATURE_LEVEL),   // number of possible feature levels
            (uint)aD3D_FEATURE_LEVEL.Length,
            D2DTools.D3D11_SDK_VERSION,
            out m_pD3D11DevicePtr,                    // returns the Direct3D device created
            out featureLevel,            // returns feature level of device created
                                         //out pD3D11DeviceContextPtr                    // returns the device immediate context
            out m_pD3D11DeviceContext
        );
        if (hr == HRESULT.S_OK)
        {
            //m_pD3D11DeviceContext = Marshal.GetObjectForIUnknown(pD3D11DeviceContextPtr) as ID3D11DeviceContext;             

            m_pDXGIDevice = Marshal.GetObjectForIUnknown(m_pD3D11DevicePtr) as IDXGIDevice1;
            if (m_pD2DFactory1 != null)
            {
                hr = m_pD2DFactory1.CreateDevice(m_pDXGIDevice, out m_pD2DDevice);
                if (hr == HRESULT.S_OK)
                {
                    hr = m_pD2DDevice.CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS.D2D1_DEVICE_CONTEXT_OPTIONS_NONE, out m_pD2DDeviceContext);
                    SafeRelease(ref m_pD2DDevice);
                }
            }
            // Marshal.Release(m_pD3D11DevicePtr);
        }
        return hr;
    }

    public static HRESULT CreateSwapChain(IntPtr hWnd)
    {
        HRESULT hr = HRESULT.S_OK;
        DXGI_SWAP_CHAIN_DESC1 swapChainDesc = new DXGI_SWAP_CHAIN_DESC1();
        swapChainDesc.Width = 1;
        swapChainDesc.Height = 1;
        swapChainDesc.Format = DXGI_FORMAT.DXGI_FORMAT_B8G8R8A8_UNORM; // this is the most common swapchain format
        swapChainDesc.Stereo = false;
        swapChainDesc.SampleDesc.Count = 1;                // don't use multi-sampling
        swapChainDesc.SampleDesc.Quality = 0;
        swapChainDesc.BufferUsage = D2DTools.DXGI_USAGE_RENDER_TARGET_OUTPUT;
        swapChainDesc.BufferCount = 2;                     // use double buffering to enable flip
        swapChainDesc.Scaling = (hWnd != IntPtr.Zero) ? DXGI_SCALING.DXGI_SCALING_NONE : DXGI_SCALING.DXGI_SCALING_STRETCH;
        swapChainDesc.SwapEffect = DXGI_SWAP_EFFECT.DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL; // all apps must use this SwapEffect       
        swapChainDesc.Flags = 0;

        IDXGIAdapter pDXGIAdapter;
        hr = m_pDXGIDevice.GetAdapter(out pDXGIAdapter);
        if (hr == HRESULT.S_OK)
        {
            IntPtr pDXGIFactory2Ptr;
            hr = pDXGIAdapter.GetParent(typeof(IDXGIFactory2).GUID, out pDXGIFactory2Ptr);
            if (hr == HRESULT.S_OK)
            {
                IDXGIFactory2 pDXGIFactory2 = Marshal.GetObjectForIUnknown(pDXGIFactory2Ptr) as IDXGIFactory2;
                if (hWnd != IntPtr.Zero)
                    hr = pDXGIFactory2.CreateSwapChainForHwnd(m_pD3D11DevicePtr, hWnd, ref swapChainDesc, IntPtr.Zero, null, out m_pDXGISwapChain1);
                else
                    hr = pDXGIFactory2.CreateSwapChainForComposition(m_pD3D11DevicePtr, ref swapChainDesc, null, out m_pDXGISwapChain1);

                hr = m_pDXGIDevice.SetMaximumFrameLatency(1);
                SafeRelease(ref pDXGIFactory2);
                Marshal.Release(pDXGIFactory2Ptr);
            }
            SafeRelease(ref pDXGIAdapter);
        }
        return hr;
    }

    public static HRESULT ConfigureSwapChain()
    {
        HRESULT hr = HRESULT.S_OK;

        //IntPtr pD3D11Texture2DPtr = IntPtr.Zero;
        //hr = m_pDXGISwapChain1.GetBuffer(0, typeof(ID3D11Texture2D).GUID, ref pD3D11Texture2DPtr);
        //m_pD3D11Texture2D = Marshal.GetObjectForIUnknown(pD3D11Texture2DPtr) as ID3D11Texture2D;

        D2D1_BITMAP_PROPERTIES1 bitmapProperties = new D2D1_BITMAP_PROPERTIES1();
        bitmapProperties.bitmapOptions = D2D1_BITMAP_OPTIONS.D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS.D2D1_BITMAP_OPTIONS_CANNOT_DRAW;
        bitmapProperties.pixelFormat = D2DTools.PixelFormat(DXGI_FORMAT.DXGI_FORMAT_B8G8R8A8_UNORM, D2D1_ALPHA_MODE.D2D1_ALPHA_MODE_IGNORE);
        //float nDpiX, nDpiY = 0.0f;
        //m_pD2DContext.GetDpi(out nDpiX, out nDpiY);
        uint nDPI = GetDpiForWindow(_hWndMain);
        bitmapProperties.dpiX = nDPI;
        bitmapProperties.dpiY = nDPI;

        IntPtr pDXGISurfacePtr = IntPtr.Zero;
        hr = m_pDXGISwapChain1.GetBuffer(0, typeof(IDXGISurface).GUID, out pDXGISurfacePtr);
        if (hr == HRESULT.S_OK)
        {
            IDXGISurface pDXGISurface = Marshal.GetObjectForIUnknown(pDXGISurfacePtr) as IDXGISurface;
            hr = m_pD2DDeviceContext.CreateBitmapFromDxgiSurface(pDXGISurface, ref bitmapProperties, out m_pD2DTargetBitmap);
            if (hr == HRESULT.S_OK)
            {
                m_pD2DDeviceContext.SetTarget(m_pD2DTargetBitmap);
            }
            SafeRelease(ref pDXGISurface);
            Marshal.Release(pDXGISurfacePtr);
        }
        return hr;
    }

    public static void Clean()
    {
        SafeRelease(ref m_pD2DDeviceContext);

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

    #region Input Handling Methods

    // Method to handle pointer movement
    public static void OnPointerMoved(float x, float y)
    {
        _d2drenderer?.OnPointerMoved(x, y);
    }

    // Method to handle pointer pressed
    public static void OnPointerPressed(float x, float y, uint button)
    {
        _d2drenderer?.OnPointerPressed(x, y, button);
    }

    // Method to handle pointer released
    public static void OnPointerReleased(float x, float y, uint button)
    {
        _d2drenderer?.OnPointerReleased(x, y, button);
    }

    // Method to handle mouse wheel events
    public static void OnMouseWheel(float deltaX, float deltaY)
    {
        _d2drenderer?.OnMouseWheel(deltaX, deltaY);
    }

    // Method to handle key down events
    public static void OnKeyDown(uint keyCode, bool ctrl, bool shift, bool alt)
    {
        _d2drenderer?.OnKeyDown(keyCode, ctrl, shift, alt);
    }

    // Method to handle key up events
    public static void OnKeyUp(uint keyCode)
    {
        _d2drenderer?.OnKeyUp(keyCode);
    }

    // Method to handle text input events
    public static void OnTextInput(string text)
    {
        _d2drenderer?.OnTextInput(text);
    }

    // Method to handle focus loss
    public static void OnBlur()
    {
        _d2drenderer?.OnBlur();
    }

    // Method to handle focus gain
    public static void OnFocus()
    {
        _d2drenderer?.OnFocus();
    }

    // Method to handle app suspension
    public static void Suspend()
    {
        _d2drenderer?.Suspend();
    }

    // Method to handle app resumption
    public static void Resume()
    {
        _d2drenderer?.Resume();
    }

    // Method to handle theme changes
    public static void SetTheme(bool isDarkMode)
    {
        _d2drenderer?.SetTheme(isDarkMode);
    }

    #endregion
}
