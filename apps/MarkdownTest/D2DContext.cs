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
using WinRT; // Required for proper WinRT interop

namespace MarkdownTest
{
    [ComImport, Guid("63aad0b8-7c24-40ff-85a8-640d944cc325"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    public interface ISwapChainPanelNative
    {
        [PreserveSig]
        HRESULT SetSwapChain(IDXGISwapChain swapChain);
    }

    // Define the D2DRenderer interface directly to bypass factory activation
    // Using the same GUID as the generated BlitzWinRT.ID2DRenderer interface
    [ComImport]
    [Guid("DFF484B2-94FA-51D1-BA2D-DC033237EC1E")]
    [InterfaceType(ComInterfaceType.InterfaceIsIInspectable)]
    public interface ID2DRenderer
    {
        void Render(string markdown);
        void Resize(uint width, uint height);
        void OnPointerMoved(float x, float y);
        void OnPointerPressed(float x, float y, uint button);
        void OnPointerReleased(float x, float y, uint button);
        void OnMouseWheel(float deltaX, float deltaY);
        void OnKeyDown(uint keyCode, bool ctrl, bool shift, bool alt);
        void OnKeyUp(uint keyCode);
        void OnTextInput(string text);
        void OnBlur();
        void OnFocus();
        void Suspend();
        void Resume();
        void SetTheme(bool isDarkMode);
    }

    public static class D2DContext
    {
        // Use the built-in WinRT classes directly instead of our own COM interop
        [DllImport("User32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
        public static extern uint GetDpiForWindow(IntPtr hwnd);

        [DllImport("Kernel32.dll", SetLastError = true, CharSet = CharSet.Auto)]
        public static extern bool QueryPerformanceFrequency(out LARGE_INTEGER lpFrequency);

        static ID2D1Factory m_pD2DFactory = null;
        static ID2D1Factory1 m_pD2DFactory1 = null;
        static IWICImagingFactory m_pWICImagingFactory = null;

        static IntPtr m_pD3D11DevicePtr = IntPtr.Zero;
        static ID3D11DeviceContext m_pD3D11DeviceContext = null;
        static IDXGIDevice1 m_pDXGIDevice = null;

        static ID2D1Device m_pD2DDevice = null;
        static ID2D1DeviceContext m_pD2DDeviceContext = null;

        static ID2D1Bitmap1 m_pD2DTargetBitmap = null;
        static IDXGISwapChain1 m_pDXGISwapChain1 = null;

        static private bool bRender = true;
        static private ulong nLastTime = 0, nTotalTime = 0;
        static private uint nNbTotalFrames = 0, nLastNbFrames = 0;
        static private IntPtr _hWndMain = IntPtr.Zero;
        static private Microsoft.UI.Windowing.AppWindow _apw;
        static private LARGE_INTEGER liFreq;

        static private SwapChainPanel _swapChainPanel = null;
        static private string _markdown = null;
        static private bool _rendered = false;

        // WinRT D2DRenderer - using the built-in generated class
        static private BlitzWinRT.D2DRenderer _d2dRenderer;
        static private bool _isActive = false;

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
            System.Diagnostics.Debug.WriteLine($"Setting up rendering with markdown content length: {markdown?.Length}");
            
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

            // First dispose of any existing renderer
            if (_d2dRenderer != null)
            {
                _d2dRenderer = null;
                // Force garbage collection to clean up COM resources
                GC.Collect();
                GC.WaitForPendingFinalizers();
            }
            
            try
            {
                // Create the D2DRenderer using the projected WinRT type in C#
                if (m_pD2DDeviceContext != null)
                {
                    // Get the native pointer as UInt64 (proper way for WinRT interop)
                    ulong contextPtr = (ulong)Marshal.GetComInterfaceForObject(m_pD2DDeviceContext, typeof(ID2D1DeviceContext)).ToInt64();
                    System.Diagnostics.Debug.WriteLine($"Creating D2DRenderer with native context: 0x{contextPtr:X}");
                    
                    try
                    {
                        // Use the standard WinRT activation via C# projection
                        // Now using the built-in BlitzWinRT.D2DRenderer class directly
                        _d2dRenderer = new BlitzWinRT.D2DRenderer(contextPtr);
                        _isActive = true;
                        System.Diagnostics.Debug.WriteLine("Successfully created WinRT D2DRenderer");
                    }
                    catch (Exception ex)
                    {
                        System.Diagnostics.Debug.WriteLine($"Error creating D2DRenderer: {ex.Message}");
                        System.Diagnostics.Debug.WriteLine($"Stack trace: {ex.StackTrace}");
                        _isActive = false;
                    }
                    
                    // Set the initial size based on the actual SwapChainPanel size
                    if (_isActive && _d2dRenderer != null && swapChainPanel.ActualWidth > 0 && swapChainPanel.ActualHeight > 0)
                    {
                        System.Diagnostics.Debug.WriteLine($"Resizing to {swapChainPanel.ActualWidth}x{swapChainPanel.ActualHeight}");
                        _d2dRenderer.Resize((uint)swapChainPanel.ActualWidth, (uint)swapChainPanel.ActualHeight);
                    }
                }
                else
                {
                    System.Diagnostics.Debug.WriteLine("Cannot create D2DRenderer: D2D device context is null");
                }
            }
            catch (Exception ex)
            {
                System.Diagnostics.Debug.WriteLine($"Critical error in SetupRendering: {ex.Message}");
                System.Diagnostics.Debug.WriteLine($"Stack trace: {ex.StackTrace}");
                _isActive = false;
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
                try
                {
                    // Clear the background to a visible color
                    // m_pD2DDeviceContext.Clear(new D2D1_COLOR_F() { r = 0.2f, g = 0.2f, b = 0.2f, a = 1.0f });

                    // Use the WinRT D2DRenderer to render markdown
                    if (_isActive && _d2dRenderer != null && _markdown != null)
                    {
                        try 
                        {
                            _d2dRenderer.Render(_markdown);
                            _rendered = true;
                        }
                        catch (Exception ex)
                        {
                            System.Diagnostics.Debug.WriteLine($"Error in WinRT rendering: {ex.Message}");
                            
                            // If we get an exception during rendering, we should recreate the renderer
                            _isActive = false;
                            _d2dRenderer = null;
                            GC.Collect();
                            GC.WaitForPendingFinalizers();
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
                catch (Exception ex)
                {
                    System.Diagnostics.Debug.WriteLine($"Render exception: {ex.Message}");
                    hr = HRESULT.E_FAIL;
                }
            }
            
            return (hr);
        }

        private static void CompositionTarget_Rendering(object sender, object e)
        {
            HRESULT hr = HRESULT.S_OK;
            // Allow continuous rendering to ensure the Rust component can animate/update as needed
            if (bRender)
            {
                Render();
                
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
                                System.Diagnostics.Debug.WriteLine($"FPS: {nNbTotalFrames}");
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
            // Disconnect event handlers
            if (_swapChainPanel != null)
            {
                _swapChainPanel.SizeChanged -= scpD2D_SizeChanged;
                CompositionTarget.Rendering -= CompositionTarget_Rendering;
            }
            
            // Release the WinRT D2DRenderer
            if (_d2dRenderer != null)
            {
                _d2dRenderer = null;
                // Force garbage collection to clean up COM resources
                GC.Collect();
                GC.WaitForPendingFinalizers();
            }
            
            _swapChainPanel = null;
            _markdown = null;
            _rendered = false;
            
            // Release Direct2D/DXGI resources
            SafeRelease(ref m_pD2DTargetBitmap);
            SafeRelease(ref m_pDXGISwapChain1);
        }

        public static HRESULT Resize(Size sz)
        {
            HRESULT hr = HRESULT.S_OK;

            if (m_pDXGISwapChain1 != null)
            {
                System.Diagnostics.Debug.WriteLine($"Resizing to {sz.Width}x{sz.Height}");
                if (m_pD2DDeviceContext != null)
                    m_pD2DDeviceContext.SetTarget(null);

                if (m_pD2DTargetBitmap != null)
                    SafeRelease(ref m_pD2DTargetBitmap);

                if (sz.Width != 0 && sz.Height != 0)
                {
                    hr = m_pDXGISwapChain1.ResizeBuffers(
                      2,
                      (uint)sz.Width,
                      (uint)sz.Height,
                      DXGI_FORMAT.DXGI_FORMAT_B8G8R8A8_UNORM,
                      0
                      );
                    
                    // Also notify the WinRT renderer about the size change
                    if (_isActive && _d2dRenderer != null)
                    {
                        try
                        {
                            _d2dRenderer.Resize((uint)sz.Width, (uint)sz.Height);
                        }
                        catch (Exception ex)
                        {
                            System.Diagnostics.Debug.WriteLine($"Error resizing D2DRenderer: {ex.Message}");
                            _isActive = false;
                            _d2dRenderer = null;
                            GC.Collect();
                            GC.WaitForPendingFinalizers();
                        }
                    }
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
                creationFlags,              
                aD3D_FEATURE_LEVEL,
                (uint)aD3D_FEATURE_LEVEL.Length,
                D2DTools.D3D11_SDK_VERSION,
                out m_pD3D11DevicePtr,                   
                out featureLevel,           
                out m_pD3D11DeviceContext
            );
            if (hr == HRESULT.S_OK)
            {
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
            }
            return hr;
        }

        public static HRESULT CreateSwapChain(IntPtr hWnd)
        {
            HRESULT hr = HRESULT.S_OK;
            DXGI_SWAP_CHAIN_DESC1 swapChainDesc = new DXGI_SWAP_CHAIN_DESC1();
            swapChainDesc.Width = 1;
            swapChainDesc.Height = 1;
            swapChainDesc.Format = DXGI_FORMAT.DXGI_FORMAT_B8G8R8A8_UNORM;
            swapChainDesc.Stereo = false;
            swapChainDesc.SampleDesc.Count = 1;                
            swapChainDesc.SampleDesc.Quality = 0;
            swapChainDesc.BufferUsage = D2DTools.DXGI_USAGE_RENDER_TARGET_OUTPUT;
            swapChainDesc.BufferCount = 2;                     
            swapChainDesc.Scaling = (hWnd != IntPtr.Zero) ? DXGI_SCALING.DXGI_SCALING_NONE : DXGI_SCALING.DXGI_SCALING_STRETCH;
            swapChainDesc.SwapEffect = DXGI_SWAP_EFFECT.DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL;       
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

            D2D1_BITMAP_PROPERTIES1 bitmapProperties = new D2D1_BITMAP_PROPERTIES1();
            bitmapProperties.bitmapOptions = D2D1_BITMAP_OPTIONS.D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS.D2D1_BITMAP_OPTIONS_CANNOT_DRAW;
            bitmapProperties.pixelFormat = D2DTools.PixelFormat(DXGI_FORMAT.DXGI_FORMAT_B8G8R8A8_UNORM, D2D1_ALPHA_MODE.D2D1_ALPHA_MODE_IGNORE);
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
            // Disconnect event handlers first
            if (_swapChainPanel != null)
            {
                _swapChainPanel.SizeChanged -= scpD2D_SizeChanged;
                CompositionTarget.Rendering -= CompositionTarget_Rendering;
            }
            
            // First dispose D2DRenderer
            if (_d2dRenderer != null)
            {
                _d2dRenderer = null;
                // Force garbage collection to clean up COM resources
                GC.Collect();
                GC.WaitForPendingFinalizers();
            }

            // Now clean up Direct2D/DirectX resources
            try
            {
                SafeRelease(ref m_pD2DDeviceContext);
                SafeRelease(ref m_pD2DTargetBitmap);
                SafeRelease(ref m_pDXGISwapChain1);
                SafeRelease(ref m_pD3D11DeviceContext);
                
                if (m_pD3D11DevicePtr != IntPtr.Zero)
                {
                    Marshal.Release(m_pD3D11DevicePtr);
                    m_pD3D11DevicePtr = IntPtr.Zero;
                }
                    
                SafeRelease(ref m_pDXGIDevice);
                SafeRelease(ref m_pWICImagingFactory);
                SafeRelease(ref m_pD2DFactory1);
                SafeRelease(ref m_pD2DFactory);
            }
            catch (Exception ex)
            {
                System.Diagnostics.Debug.WriteLine($"Error releasing DirectX resources: {ex.Message}");
            }
            
            // Final cleanup
            _swapChainPanel = null;
            _markdown = null;
            _rendered = false;
            
            // Force a final GC
            GC.Collect();
            GC.WaitForPendingFinalizers();
        }

        #region Input Handling Methods

        // Method to handle pointer movement
        public static void OnPointerMoved(float x, float y)
        {
            if (_isActive && _d2dRenderer != null)
            {
                try
                {
                    _d2dRenderer.OnPointerMoved(x, y);
                }
                catch (Exception ex)
                {
                    System.Diagnostics.Debug.WriteLine($"Error in OnPointerMoved: {ex.Message}");
                    _isActive = false;
                }
            }
        }

        // Method to handle pointer pressed
        public static void OnPointerPressed(float x, float y, uint button)
        {
            if (_isActive && _d2dRenderer != null)
            {
                try
                {
                    _d2dRenderer.OnPointerPressed(x, y, button);
                }
                catch (Exception ex)
                {
                    System.Diagnostics.Debug.WriteLine($"Error in OnPointerPressed: {ex.Message}");
                    _isActive = false;
                }
            }
        }

        // Method to handle pointer released
        public static void OnPointerReleased(float x, float y, uint button)
        {
            if (_isActive && _d2dRenderer != null)
            {
                try
                {
                    _d2dRenderer.OnPointerReleased(x, y, button);
                }
                catch (Exception ex)
                {
                    System.Diagnostics.Debug.WriteLine($"Error in OnPointerReleased: {ex.Message}");
                    _isActive = false;
                }
            }
        }

        // Method to handle mouse wheel events
        public static void OnMouseWheel(float deltaX, float deltaY)
        {
            if (_isActive && _d2dRenderer != null)
            {
                try
                {
                    _d2dRenderer.OnMouseWheel(deltaX, deltaY);
                }
                catch (Exception ex)
                {
                    System.Diagnostics.Debug.WriteLine($"Error in OnMouseWheel: {ex.Message}");
                    _isActive = false;
                }
            }
        }

        // Method to handle key down events
        public static void OnKeyDown(uint keyCode, bool ctrl, bool shift, bool alt)
        {
            if (_isActive && _d2dRenderer != null)
            {
                try
                {
                    _d2dRenderer.OnKeyDown(keyCode, ctrl, shift, alt);
                }
                catch (Exception ex)
                {
                    System.Diagnostics.Debug.WriteLine($"Error in OnKeyDown: {ex.Message}");
                    _isActive = false;
                }
            }
        }

        // Method to handle key up events
        public static void OnKeyUp(uint keyCode)
        {
            if (_isActive && _d2dRenderer != null)
            {
                try
                {
                    _d2dRenderer.OnKeyUp(keyCode);
                }
                catch (Exception ex)
                {
                    System.Diagnostics.Debug.WriteLine($"Error in OnKeyUp: {ex.Message}");
                    _isActive = false;
                }
            }
        }

        // Method to handle text input events
        public static void OnTextInput(string text)
        {
            if (_isActive && _d2dRenderer != null)
            {
                try
                {
                    _d2dRenderer.OnTextInput(text);
                }
                catch (Exception ex)
                {
                    System.Diagnostics.Debug.WriteLine($"Error in OnTextInput: {ex.Message}");
                    _isActive = false;
                }
            }
        }

        // Method to handle focus loss
        public static void OnBlur()
        {
            if (_isActive && _d2dRenderer != null)
            {
                try
                {
                    _d2dRenderer.OnBlur();
                }
                catch (Exception ex)
                {
                    System.Diagnostics.Debug.WriteLine($"Error in OnBlur: {ex.Message}");
                    _isActive = false;
                }
            }
        }

        // Method to handle focus gain
        public static void OnFocus()
        {
            if (_isActive && _d2dRenderer != null)
            {
                try
                {
                    _d2dRenderer.OnFocus();
                }
                catch (Exception ex)
                {
                    System.Diagnostics.Debug.WriteLine($"Error in OnFocus: {ex.Message}");
                    _isActive = false;
                }
            }
        }

        // Method to handle theme changes
        public static void SetTheme(bool isDarkMode)
        {
            if (_isActive && _d2dRenderer != null)
            {
                try
                {
                    _d2dRenderer.SetTheme(isDarkMode);
                }
                catch (Exception ex)
                {
                    System.Diagnostics.Debug.WriteLine($"Error in SetTheme: {ex.Message}");
                    _isActive = false;
                }
            }
        }

        // Method to handle app suspension
        public static void Suspend()
        {
            if (_isActive && _d2dRenderer != null)
            {
                try
                {
                    _d2dRenderer.Suspend();
                }
                catch (Exception ex)
                {
                    System.Diagnostics.Debug.WriteLine($"Error in Suspend: {ex.Message}");
                    _isActive = false;
                }
            }
        }

        // Method to handle app resumption
        public static void Resume()
        {
            if (_isActive && _d2dRenderer != null)
            {
                try
                {
                    _d2dRenderer.Resume();
                }
                catch (Exception ex)
                {
                    System.Diagnostics.Debug.WriteLine($"Error in Resume: {ex.Message}");
                    _isActive = false;
                }
            }
        }

        #endregion
    }
}
