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
using WinRT;
using System.Collections.Generic; // Required for proper WinRT interop

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
        void Tick();
    }

    public static class D2DContext
    {
        // Use the built-in WinRT classes directly instead of our own COM interop
        [DllImport("User32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
        public static extern uint GetDpiForWindow(IntPtr hwnd);

        [DllImport("Kernel32.dll", SetLastError = true, CharSet = CharSet.Auto)]
        public static extern bool QueryPerformanceFrequency(out LARGE_INTEGER lpFrequency);

        [DllImport("Libraries/BlitzWinRT.dll", CharSet = CharSet.Unicode, CallingConvention = CallingConvention.StdCall)]
        internal static extern Int32 CreateD2DRenderer(ulong deviceContextPtr, out IntPtr class_instance);

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
        static private BlitzWinRT.ILogger _d2dLogger; // Strong reference to prevent GC from collecting it
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
            
            // First ensure a proper cleanup to avoid resource conflicts
            UnloadPage();
            
            // Force garbage collection before creating new resources
            GC.Collect();
            GC.WaitForPendingFinalizers();
            
            _rendered = false;
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
                // Create the D2DRenderer using the projected WinRT type in C#
                if (m_pD2DDeviceContext != null)
                {
                    // Get the native pointer as UInt64 (proper way for WinRT interop)
                    ulong contextPtr = (ulong)Marshal.GetComInterfaceForObject(m_pD2DDeviceContext, typeof(ID2D1DeviceContext)).ToInt64();
                    System.Diagnostics.Debug.WriteLine($"Creating D2DRenderer with native context: 0x{contextPtr:X}");
                    
                    try
                    {
                        _d2dRenderer = new BlitzWinRT.D2DRenderer(contextPtr);
                        
                        // Create and attach our logger to the renderer
                        _d2dLogger = new D2DLogger(isVerbose: true);
                        System.Diagnostics.Debug.WriteLine("Attaching logger to D2DRenderer");
                        _d2dRenderer.SetLogger(_d2dLogger);
                        
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
                    // Instead of rendering markdown directly here, we just mark that markdown content is available
                    // The actual rendering will happen in the CompositionTarget_Rendering method via Tick()
                    if (_isActive && _d2dRenderer != null && _markdown != null && !_rendered)
                    {
                        try 
                        {
                            System.Diagnostics.Debug.WriteLine($"Rendering markdown content of length: {_markdown.Length}");
                            
                            // Add a small delay before initial render to ensure the DOM is fully initialized
                            System.Threading.Thread.Sleep(50);
                            
                            _d2dRenderer.Render(_markdown);
                            _rendered = true;
                            
                            // After successful render, add another small delay before ticking 
                            // to ensure everything is properly set up
                            System.Threading.Thread.Sleep(16);
                        }
                        catch (Exception ex)
                        {
                            System.Diagnostics.Debug.WriteLine($"Error in WinRT rendering: {ex.Message}");
                            System.Diagnostics.Debug.WriteLine($"Stack trace: {ex.StackTrace}");
                            
                            // If we get an exception during rendering, we should recreate the renderer
                            _isActive = false;
                            _d2dRenderer = null;
                            GC.Collect();
                            GC.WaitForPendingFinalizers();
                        }
                    }
                    
                    // No need to present the swap chain here as it's done in CompositionTarget_Rendering
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
            
            if (!bRender || !_isActive || _d2dRenderer == null)
            {
                return;
            }
            
            try
            {
                // First, ensure we have properly configured Direct2D resources
                if (m_pD2DTargetBitmap == null && m_pDXGISwapChain1 != null)
                {
                    // If target bitmap is missing but we have a swap chain, try to reconfigure
                    System.Diagnostics.Debug.WriteLine("Attempting to reconfigure swap chain due to missing target bitmap");
                    hr = ConfigureSwapChain();
                    if (hr != HRESULT.S_OK)
                    {
                        System.Diagnostics.Debug.WriteLine($"Failed to reconfigure swap chain: 0x{hr:X}");
                        return;
                    }
                }
                
                // Now proceed with rendering if everything is properly configured
                bool needsPresent = false;

                // For the first frame or if not yet rendered, make sure to show something
                if (!_rendered)
                {
                    try 
                    {
                        // Clear with white background to give immediate visual feedback
                        m_pD2DDeviceContext.BeginDraw();
                        m_pD2DDeviceContext.Clear(new D2D1_COLOR_F() { r = 1.0f, g = 1.0f, b = 1.0f, a = 1.0f });
                        
                        UInt64 tag1 = 0, tag2 = 0;
                        m_pD2DDeviceContext.EndDraw(out tag1, out tag2);
                        
                        needsPresent = true;
                        System.Diagnostics.Debug.WriteLine("Cleared background to white");
                    }
                    catch (Exception ex)
                    {
                        System.Diagnostics.Debug.WriteLine($"Error during initial render clear: {ex.Message}");
                        try { m_pD2DDeviceContext.EndDraw(out UInt64 _, out UInt64 _); } catch { }
                    }
                }
                
                // Call Tick, which will render only if content has changed
                //System.Diagnostics.Debug.WriteLine("Calling Tick() on D2DRenderer");
                try
                {
                    // Use a try-catch to handle any exceptions from Tick, including WinRT exceptions
                    _d2dRenderer.Tick();
                    //System.Diagnostics.Debug.WriteLine("Tick completed successfully");
                }
                catch (Exception ex)
                {
                    System.Diagnostics.Debug.WriteLine($"Exception in Tick: {ex.Message}");
                    
                    // Don't fail immediately, allow the swap chain to still present the last 
                    // successfully rendered frame if we have one
                    if (!_rendered)
                    {
                        // If we haven't successfully rendered anything yet, we need to 
                        // avoid presenting altogether
                        needsPresent = false;
                    }
                }
                
                // Always present after Tick since we can't know if Rust side actually rendered anything
                needsPresent = true;
                
                // Present the swap chain
                if (needsPresent && m_pDXGISwapChain1 != null)
                {
                    //System.Diagnostics.Debug.WriteLine("Presenting swap chain");
                    hr = m_pDXGISwapChain1.Present(1, 0); // Use vsync (1) for smoother rendering
                    
                    if (hr == HRESULT.S_OK)
                    {
                        _rendered = true;
                        //System.Diagnostics.Debug.WriteLine("Swap chain presented successfully");
                    }
                    else if ((uint)hr == D2DTools.D2DERR_RECREATE_TARGET)
                    {
                        System.Diagnostics.Debug.WriteLine("Need to recreate rendering target");
                        if (m_pD2DDeviceContext != null)
                            m_pD2DDeviceContext.SetTarget(null);
                        
                        SafeRelease(ref m_pD2DTargetBitmap);
                        
                        hr = CreateSwapChain(IntPtr.Zero);
                        if (hr == HRESULT.S_OK)
                        {
                            hr = ConfigureSwapChain();
                            _rendered = false;
                        }
                    }
                    else
                    {
                        System.Diagnostics.Debug.WriteLine($"Present failed with HRESULT: 0x{hr:X}");
                    }
                }
            }
            catch (Exception ex)
            {
                System.Diagnostics.Debug.WriteLine($"Error in CompositionTarget_Rendering: {ex.Message} (0x{ex.HResult:X8})");
                
                // Try to recover from specific D2D errors
                if (ex.HResult == unchecked((int)0xEE093000) || 
                    ex.HResult == unchecked((int)0xC994A000) ||
                    ex.HResult == unchecked((int)0x88990011)) // DXGI_ERROR_DEVICE_REMOVED
                {
                    try
                    {
                        System.Diagnostics.Debug.WriteLine("Attempting to recover from D2D error");
                        
                        // Set target to null before releasing bitmap
                        if (m_pD2DDeviceContext != null)
                            m_pD2DDeviceContext.SetTarget(null);
                            
                        SafeRelease(ref m_pD2DTargetBitmap);
                        
                        // Wait a bit before recreating resources
                        System.Threading.Thread.Sleep(50);
                        
                        if (m_pDXGISwapChain1 != null)
                        {
                            hr = ConfigureSwapChain();
                            if (hr == HRESULT.S_OK)
                            {
                                System.Diagnostics.Debug.WriteLine("Resource recovery successful");
                                _rendered = false; // Force a redraw
                            }
                        }
                    }
                    catch (Exception recovery_ex)
                    {
                        System.Diagnostics.Debug.WriteLine($"Recovery failed: {recovery_ex.Message}");
                        _isActive = false;
                    }
                }
                else
                {
                    // For other errors, deactivate the renderer
                    System.Diagnostics.Debug.WriteLine($"Deactivating renderer due to unrecoverable error");
                    _isActive = false;
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
            
            // First suspend the renderer to avoid any rendering while we're cleaning up
            if (_isActive && _d2dRenderer != null)
            {
                try
                {
                    _d2dRenderer.Suspend();
                }
                catch (Exception ex)
                {
                    System.Diagnostics.Debug.WriteLine($"Error suspending D2DRenderer: {ex.Message}");
                }
            }
            
            // Release the WinRT D2DRenderer
            if (_d2dRenderer != null)
            {
                // Clear the references
                _d2dRenderer = null;
                _d2dLogger = null; // Clear the logger reference as well
                _isActive = false;
                
                // Force garbage collection to release COM resources
                GC.Collect();
                GC.WaitForPendingFinalizers();
            }
            
            // Reset state variables
            _swapChainPanel = null;
            _markdown = null;
            _rendered = false;
            
            // Clear Direct2D resources to ensure clean slate for next page
            if (m_pD2DDeviceContext != null)
            {
                // Need to set the target to null before releasing bitmap
                m_pD2DDeviceContext.SetTarget(null);
            }
            
            SafeRelease(ref m_pD2DTargetBitmap);
            
            // Release swapchain last, after ensuring no references remain
            if (m_pDXGISwapChain1 != null)
            {
                // Release outstanding buffer references by presenting with DXGI_PRESENT_DO_NOT_WAIT flag
                // This flag will discard the current frame without waiting, which effectively
                // releases references to the swapchain buffers
                try
                {
                    m_pDXGISwapChain1.Present(0, DXGITools.DXGI_PRESENT_DO_NOT_WAIT);
                }
                catch (Exception ex)
                {
                    System.Diagnostics.Debug.WriteLine($"Error discarding swapchain buffers: {ex.Message}");
                }
                
                SafeRelease(ref m_pDXGISwapChain1);
            }
            
            // Final GC to ensure everything is cleaned up
            GC.Collect();
            GC.WaitForPendingFinalizers();
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
            
            // First ensure any existing swapchain is properly released
            if (m_pDXGISwapChain1 != null)
            {
                try
                {
                    // Release outstanding buffer references
                    m_pDXGISwapChain1.Present(0, DXGITools.DXGI_PRESENT_DO_NOT_WAIT);
                    
                    // Set the target to null to release reference to the bitmap
                    if (m_pD2DDeviceContext != null)
                    {
                        m_pD2DDeviceContext.SetTarget(null);
                    }
                    
                    // Release the swap chain
                    SafeRelease(ref m_pDXGISwapChain1);
                    
                    // Force garbage collection to ensure all references are released
                    GC.Collect();
                    GC.WaitForPendingFinalizers();
                }
                catch (Exception ex)
                {
                    System.Diagnostics.Debug.WriteLine($"Error releasing old swapchain: {ex.Message}");
                }
            }
            
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
            
            // First ensure any existing target bitmap is released
            if (m_pD2DTargetBitmap != null)
            {
                if (m_pD2DDeviceContext != null)
                {
                    try
                    {
                        m_pD2DDeviceContext.SetTarget(null);
                    }
                    catch (Exception ex)
                    {
                        System.Diagnostics.Debug.WriteLine($"Error clearing target: {ex.Message}");
                    }
                }
                SafeRelease(ref m_pD2DTargetBitmap);
            }
            
            // Make sure swap chain is valid
            if (m_pDXGISwapChain1 == null)
            {
                System.Diagnostics.Debug.WriteLine("Cannot configure swap chain: swap chain is null");
                return HRESULT.E_POINTER;
            }
            
            // Make sure device context is valid
            if (m_pD2DDeviceContext == null)
            {
                System.Diagnostics.Debug.WriteLine("Cannot configure swap chain: device context is null");
                return HRESULT.E_POINTER;
            }
            
            // Wait for GPU to finish all operations before trying to access the swap chain buffer
            System.Threading.Thread.Sleep(50);
            
            D2D1_BITMAP_PROPERTIES1 bitmapProperties = new D2D1_BITMAP_PROPERTIES1();
            bitmapProperties.bitmapOptions = D2D1_BITMAP_OPTIONS.D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS.D2D1_BITMAP_OPTIONS_CANNOT_DRAW;
            bitmapProperties.pixelFormat = D2DTools.PixelFormat(DXGI_FORMAT.DXGI_FORMAT_B8G8R8A8_UNORM, D2D1_ALPHA_MODE.D2D1_ALPHA_MODE_PREMULTIPLIED);
            uint nDPI = GetDpiForWindow(_hWndMain);
            if (nDPI == 0) nDPI = 96; // Use default DPI if window handle is invalid
            bitmapProperties.dpiX = nDPI;
            bitmapProperties.dpiY = nDPI;

            try
            {
                IntPtr pDXGISurfacePtr = IntPtr.Zero;
                hr = m_pDXGISwapChain1.GetBuffer(0, typeof(IDXGISurface).GUID, out pDXGISurfacePtr);
                
                if (hr == HRESULT.S_OK && pDXGISurfacePtr != IntPtr.Zero)
                {
                    System.Diagnostics.Debug.WriteLine("Successfully acquired swap chain buffer");
                    
                    IDXGISurface pDXGISurface = Marshal.GetObjectForIUnknown(pDXGISurfacePtr) as IDXGISurface;
                    
                    if (pDXGISurface != null)
                    {
                        hr = m_pD2DDeviceContext.CreateBitmapFromDxgiSurface(pDXGISurface, ref bitmapProperties, out m_pD2DTargetBitmap);
                        
                        if (hr == HRESULT.S_OK && m_pD2DTargetBitmap != null)
                        {
                            System.Diagnostics.Debug.WriteLine("Successfully created target bitmap from DXGI surface");
                            m_pD2DDeviceContext.SetTarget(m_pD2DTargetBitmap);
                            
                            // Clear with white background to give immediate visual feedback
                            m_pD2DDeviceContext.BeginDraw();
                            m_pD2DDeviceContext.Clear(new D2D1_COLOR_F() { r = 1.0f, g = 1.0f, b = 1.0f, a = 1.0f });
                            
                            UInt64 tag1 = 0, tag2 = 0;
                            hr = m_pD2DDeviceContext.EndDraw(out tag1, out tag2);
                            
                            if (hr != HRESULT.S_OK)
                            {
                                System.Diagnostics.Debug.WriteLine($"EndDraw failed in ConfigureSwapChain: 0x{hr:X}");
                            }
                            
                            // Present immediately to show something on screen
                            hr = m_pDXGISwapChain1.Present(1, 0);
                            if (hr != HRESULT.S_OK)
                            {
                                System.Diagnostics.Debug.WriteLine($"Present failed in ConfigureSwapChain: 0x{hr:X}");
                            }
                        }
                        else
                        {
                            System.Diagnostics.Debug.WriteLine($"Failed to create target bitmap: 0x{hr:X}");
                        }
                        
                        SafeRelease(ref pDXGISurface);
                    }
                    else
                    {
                        System.Diagnostics.Debug.WriteLine("Failed to get IDXGISurface from pointer");
                        hr = HRESULT.E_NOINTERFACE;
                    }
                    
                    Marshal.Release(pDXGISurfacePtr);
                }
                else
                {
                    System.Diagnostics.Debug.WriteLine($"Failed to get swap chain buffer: 0x{hr:X}");
                }
            }
            catch (Exception ex)
            {
                System.Diagnostics.Debug.WriteLine($"Error configuring swap chain: {ex.Message} (0x{ex.HResult:X8})");
                hr = HRESULT.E_FAIL;
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
            
            // First dispose D2DRenderer and logger
            if (_d2dRenderer != null || _d2dLogger != null)
            {
                _d2dRenderer = null;
                _d2dLogger = null;
                
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

    // Logger implementation for our WinRT interface
    public class D2DLogger : BlitzWinRT.ILogger
    {
        private int _messageCounter = 0;
        private bool _isVerbose;
        private static List<string> _logBuffer = new List<string>();
        private static readonly int MaxBufferSize = 1000;
        
        public D2DLogger(bool isVerbose = true)
        {
            _isVerbose = isVerbose;
            LogMessage("D2DLogger created");
        }
        
        public void LogMessage(string message)
        {
            try
            {
                int counter = System.Threading.Interlocked.Increment(ref _messageCounter);
                string timestampedMessage = $"[{counter}] {DateTime.Now.ToString("HH:mm:ss.fff")} - {message}";
                
                // Always store in buffer for potential diagnostic retrieval
                lock (_logBuffer)
                {
                    _logBuffer.Add(timestampedMessage);
                    if (_logBuffer.Count > MaxBufferSize)
                    {
                        _logBuffer.RemoveAt(0);
                    }
                }
                
                // Always output to debug console for immediate visibility
                System.Diagnostics.Debug.WriteLine($"[RUST] {timestampedMessage}");
            }
            catch (Exception ex)
            {
                // Output directly to console if there's an issue with the logger itself
                System.Diagnostics.Debug.WriteLine($"Error in logger: {ex.Message}");
                Console.WriteLine($"Error in logger: {ex.Message}");
            }
        }
        
        // Get all logs as a single string for diagnostic purposes
        public static string GetAllLogs()
        {
            lock (_logBuffer)
            {
                return string.Join(Environment.NewLine, _logBuffer);
            }
        }
        
        // Clear log buffer
        public static void ClearLogs()
        {
            lock (_logBuffer)
            {
                _logBuffer.Clear();
            }
        }
    }
}
