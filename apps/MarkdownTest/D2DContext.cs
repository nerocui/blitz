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
                    
                    // Set the initial size based on the SwapChainPanel size
                    if (_isActive && _d2dRenderer != null)
                    {
                        // Get panel size or use fallback dimensions if ActualWidth/Height are 0
                        double width = swapChainPanel.ActualWidth;
                        double height = swapChainPanel.ActualHeight;
                        
                        // If ActualWidth/Height are 0, try to use requested width/height
                        if (width <= 0 || height <= 0)
                        {
                            width = swapChainPanel.Width;
                            height = swapChainPanel.Height;
                            System.Diagnostics.Debug.WriteLine($"Using requested size: {width}x{height}");
                        }
                        
                        // If still 0, try to get size from parent container
                        if (width <= 0 || height <= 0)
                        {
                            var parent = VisualTreeHelper.GetParent(swapChainPanel) as FrameworkElement;
                            if (parent != null)
                            {
                                width = parent.ActualWidth;
                                height = parent.ActualHeight;
                                System.Diagnostics.Debug.WriteLine($"Using parent size: {width}x{height}");
                            }
                        }
                        
                        // If all else fails, use default minimum dimensions
                        if (width <= 0 || height <= 0)
                        {
                            width = 800;
                            height = 600;
                            System.Diagnostics.Debug.WriteLine($"Using default size: {width}x{height}");
                        }
                        
                        // Ensure size is at least 1x1 to avoid rendering errors
                        width = Math.Max(1, width);
                        height = Math.Max(1, height);
                        
                        System.Diagnostics.Debug.WriteLine($"Resizing to {width}x{height}");
                        _d2dRenderer.Resize((uint)width, (uint)height);
                        
                        // Schedule an additional resize when layout is complete
                        if (swapChainPanel.ActualWidth <= 0 || swapChainPanel.ActualHeight <= 0)
                        {
                            swapChainPanel.Loaded += (s, e) => 
                            {
                                if (_isActive && _d2dRenderer != null && 
                                    swapChainPanel.ActualWidth > 0 && swapChainPanel.ActualHeight > 0)
                                {
                                    System.Diagnostics.Debug.WriteLine($"Post-load resize to {swapChainPanel.ActualWidth}x{swapChainPanel.ActualHeight}");
                                    _d2dRenderer.Resize((uint)swapChainPanel.ActualWidth, (uint)swapChainPanel.ActualHeight);
                                }
                            };
                        }
                        
                        // CRITICAL FIX: Explicitly call Render immediately after setup to initialize content
                        if (_markdown != null)
                        {
                            System.Diagnostics.Debug.WriteLine($"Explicitly calling Render with markdown content length: {_markdown.Length}");
                            Render();
                        }
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
                // First, ensure we have properly configured Direct2D resources and a valid target
                bool targetIsNull = m_pD2DTargetBitmap == null;
                bool targetNeedsReconfiguring = targetIsNull && m_pDXGISwapChain1 != null;
                
                if (targetNeedsReconfiguring)
                {
                    // If target bitmap is missing but we have a swap chain, try to reconfigure
                    System.Diagnostics.Debug.WriteLine("Attempting to reconfigure swap chain due to missing target bitmap");
                    hr = ConfigureSwapChain();
                    if (hr != HRESULT.S_OK)
                    {
                        System.Diagnostics.Debug.WriteLine($"Failed to reconfigure swap chain: 0x{hr:X}");
                        // Don't attempt to render if we couldn't configure the swap chain
                        return;
                    }
                    else
                    {
                        // Check if the target was successfully created
                        targetIsNull = m_pD2DTargetBitmap == null;
                        if (targetIsNull)
                        {
                            System.Diagnostics.Debug.WriteLine("Target bitmap is still null after reconfiguring swap chain");
                            return;
                        }
                    }
                }
                
                // Extra validation to prevent NULL target rendering
                if (targetIsNull)
                {
                    System.Diagnostics.Debug.WriteLine("Target bitmap is null - cannot render");
                    return; 
                }
                
                // Ensure the device context has a valid target set
                try
                {
                    // Set the target if we have one but it's not currently set
                    if (m_pD2DTargetBitmap != null)
                    {
                        // Get the current target
                        ID2D1Image currentTarget = null;
                        m_pD2DDeviceContext.GetTarget(out currentTarget);
                        
                        bool needsTargetReset = currentTarget == null;
                        
                        // Clean up the current target reference
                        if (currentTarget != null)
                        {
                            SafeRelease(ref currentTarget);
                        }
                        
                        // Set the target if needed
                        if (needsTargetReset)
                        {
                            System.Diagnostics.Debug.WriteLine("Setting the target bitmap on device context");
                            m_pD2DDeviceContext.SetTarget(m_pD2DTargetBitmap);
                        }
                    }
                }
                catch (Exception targetEx)
                {
                    System.Diagnostics.Debug.WriteLine($"Error checking/setting target: {targetEx.Message}");
                    return;
                }
                
                // Now proceed with rendering if everything is properly configured
                bool needsPresent = false;

                // For the first frame or if not yet rendered, make sure to show something
                if (!_rendered && _d2dRenderer == null)
                {
                    try 
                    {
                        // Verify target is set before clearing
                        ID2D1Image checkTarget = null;
                        m_pD2DDeviceContext.GetTarget(out checkTarget);
                        
                        if (checkTarget != null)
                        {
                            // Clear with white background to give immediate visual feedback
                            m_pD2DDeviceContext.BeginDraw();
                            m_pD2DDeviceContext.Clear(new D2D1_COLOR_F() { r = 1.0f, g = 1.0f, b = 1.0f, a = 1.0f });
                            
                            UInt64 tag1 = 0, tag2 = 0;
                            m_pD2DDeviceContext.EndDraw(out tag1, out tag2);
                            
                            needsPresent = true;
                            System.Diagnostics.Debug.WriteLine("Cleared background to white");
                            
                            SafeRelease(ref checkTarget);
                        }
                        else
                        {
                            System.Diagnostics.Debug.WriteLine("WARNING: Cannot clear - NULL target is set");
                            // Try one more time to set the target
                            if (m_pD2DTargetBitmap != null)
                            {
                                m_pD2DDeviceContext.SetTarget(m_pD2DTargetBitmap);
                                System.Diagnostics.Debug.WriteLine("Re-attempted to set target bitmap");
                            }
                        }
                    }
                    catch (Exception ex)
                    {
                        System.Diagnostics.Debug.WriteLine($"Error during initial render clear: {ex.Message}");
                        try { m_pD2DDeviceContext.EndDraw(out UInt64 _, out UInt64 _); } catch { }
                    }
                }
                
                // Before calling Tick, ensure the target is still valid
                bool canTickSafely = false;
                try
                {
                    ID2D1Image preTickTarget = null;
                    m_pD2DDeviceContext.GetTarget(out preTickTarget);
                    canTickSafely = preTickTarget != null;
                    if (preTickTarget != null)
                    {
                        SafeRelease(ref preTickTarget);
                    }
                }
                catch (Exception ex)
                {
                    System.Diagnostics.Debug.WriteLine($"Error checking target before Tick: {ex.Message}");
                    canTickSafely = false;
                }
                
                // Call Tick, which will render only if content has changed
                if (canTickSafely)
                {
                    try
                    {
                        // Use a try-catch to handle any exceptions from Tick, including WinRT exceptions
                        _d2dRenderer.Tick();
                        needsPresent = true; // We should present after a successful Tick
                    }
                    catch (Exception ex)
                    {
                        System.Diagnostics.Debug.WriteLine($"Exception in Tick: {ex.Message}");
                        
                        // If we hit a D2D error about NULL target, try to recover
                        if (ex.Message.Contains("NULL target"))
                        {
                            System.Diagnostics.Debug.WriteLine("Attempting to recover from NULL target error");
                            // Try to reset target and reconfigure
                            if (m_pD2DDeviceContext != null)
                            {
                                m_pD2DDeviceContext.SetTarget(null);
                                SafeRelease(ref m_pD2DTargetBitmap);
                                hr = ConfigureSwapChain();
                                if (hr == HRESULT.S_OK)
                                {
                                    System.Diagnostics.Debug.WriteLine("Successfully reconfigured after NULL target error");
                                }
                            }
                        }
                    }
                }
                else
                {
                    System.Diagnostics.Debug.WriteLine("Skipping Tick due to NULL target");
                }
                
                // Only present if we have a valid swap chain and performed some rendering
                if (needsPresent && m_pDXGISwapChain1 != null)
                {
                    // One final check to ensure target is set before presenting
                    try
                    {
                        ID2D1Image finalTarget = null;
                        m_pD2DDeviceContext.GetTarget(out finalTarget);
                        
                        if (finalTarget != null)
                        {
                            SafeRelease(ref finalTarget);
                            hr = m_pDXGISwapChain1.Present(1, 0); // Use vsync (1) for smoother rendering
                            
                            if (hr == HRESULT.S_OK)
                            {
                                _rendered = true;
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
                        else
                        {
                            System.Diagnostics.Debug.WriteLine("Skipping Present due to NULL target");
                        }
                    }
                    catch (Exception ex)
                    {
                        System.Diagnostics.Debug.WriteLine($"Error during final target check or present: {ex.Message}");
                    }
                }
            }
            catch (Exception ex)
            {
                System.Diagnostics.Debug.WriteLine($"Error in CompositionTarget_Rendering: {ex.Message} (0x{ex.HResult:X8})");
                
                // Try to recover from specific D2D errors
                if (ex.HResult == unchecked((int)0xEE093000) || 
                    ex.HResult == unchecked((int)0xC994A000) ||
                    ex.HResult == unchecked((int)0x88990011) || // DXGI_ERROR_DEVICE_REMOVED
                    ex.HResult == unchecked((int)0x8077C548))   // D2D error code from log
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
                    System.Diagnostics.Debug.WriteLine($"Deactivating renderer due to unrecoverable error: 0x{ex.HResult:X8}");
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
                // Get original swap chain dimensions for debugging
                DXGI_SWAP_CHAIN_DESC1 origDesc = new DXGI_SWAP_CHAIN_DESC1();
                try
                {
                    hr = m_pDXGISwapChain1.GetDesc1(out origDesc);
                    if (hr == HRESULT.S_OK)
                    {
                        System.Diagnostics.Debug.WriteLine($"[DEBUG] Resize: Original swap chain dimensions: {origDesc.Width}x{origDesc.Height}");
                        System.Diagnostics.Debug.WriteLine($"[DEBUG] Resize: Requested dimensions: {sz.Width}x{sz.Height}");
                    }
                }
                catch (Exception ex)
                {
                    System.Diagnostics.Debug.WriteLine($"[DEBUG] Error getting swap chain description: {ex.Message}");
                }

                System.Diagnostics.Debug.WriteLine($"[DEBUG] Resizing to {sz.Width}x{sz.Height}");

                // Properly release all buffer references before resizing
                try
                {
                    // First, set D2D target to null
                    if (m_pD2DDeviceContext != null)
                    {
                        try
                        {
                            m_pD2DDeviceContext.SetTarget(null);
                        }
                        catch (Exception ex)
                        {
                            System.Diagnostics.Debug.WriteLine($"[DEBUG] Error clearing target in Resize: {ex.Message}");
                        }
                    }

                    // Release D2D target bitmap
                    if (m_pD2DTargetBitmap != null)
                    {
                        SafeRelease(ref m_pD2DTargetBitmap);
                    }

                    // Force a present to flush any pending operations
                    try
                    {
                        m_pDXGISwapChain1.Present(0, DXGITools.DXGI_PRESENT_DO_NOT_WAIT);
                    }
                    catch (Exception ex)
                    {
                        System.Diagnostics.Debug.WriteLine($"[DEBUG] Non-critical error during buffer flush: {ex.Message}");
                    }

                    // Force GC to clean up any references that might be held by the runtime
                    GC.Collect();
                    GC.WaitForPendingFinalizers();
                    
                    // Wait a small amount of time to ensure GPU operations complete
                    System.Threading.Thread.Sleep(50);
                }
                catch (Exception ex)
                {
                    System.Diagnostics.Debug.WriteLine($"[DEBUG] Exception during pre-resize cleanup: {ex.Message}");
                }

                if (sz.Width > 0 && sz.Height > 0) // Changed from != 0 to > 0 to be more explicit
                {
                    try
                    {
                        hr = m_pDXGISwapChain1.ResizeBuffers(
                            2,
                            (uint)sz.Width,
                            (uint)sz.Height,
                            DXGI_FORMAT.DXGI_FORMAT_B8G8R8A8_UNORM,
                            0
                        );

                        System.Diagnostics.Debug.WriteLine($"[DEBUG] ResizeBuffers result: 0x{hr:X}");

                        // If buffer references still exist, try more aggressive cleanup
                        if ((uint)hr == 0x887A0001) // DXGI_ERROR_INVALID_CALL (buffer references exist)
                        {
                            System.Diagnostics.Debug.WriteLine("[DEBUG] Buffer references still exist, attempting more aggressive cleanup");
                            
                            // Release everything
                            if (m_pD2DDeviceContext != null)
                            {
                                m_pD2DDeviceContext.SetTarget(null);
                            }
                            SafeRelease(ref m_pD2DTargetBitmap);
                            
                            // Force another GC cycle
                            GC.Collect();
                            GC.WaitForPendingFinalizers();
                            System.Threading.Thread.Sleep(100);
                            
                            // Try resize again
                            hr = m_pDXGISwapChain1.ResizeBuffers(
                                2,
                                (uint)sz.Width,
                                (uint)sz.Height,
                                DXGI_FORMAT.DXGI_FORMAT_B8G8R8A8_UNORM,
                                0
                            );
                            
                            System.Diagnostics.Debug.WriteLine($"[DEBUG] ResizeBuffers second attempt result: 0x{hr:X}");
                            
                            // If still failing, recreate the swap chain
                            if (hr != HRESULT.S_OK)
                            {
                                System.Diagnostics.Debug.WriteLine("[DEBUG] Recreating swap chain due to persistent buffer references");
                                SafeRelease(ref m_pDXGISwapChain1);
                                
                                hr = CreateSwapChain(IntPtr.Zero);
                                if (hr == HRESULT.S_OK)
                                {
                                    System.Diagnostics.Debug.WriteLine("[DEBUG] Successfully recreated swap chain");
                                }
                                else
                                {
                                    System.Diagnostics.Debug.WriteLine($"[DEBUG] Failed to recreate swap chain: 0x{hr:X}");
                                    return hr;
                                }
                            }
                        }

                        // Verify the resize was successful by checking the new dimensions
                        DXGI_SWAP_CHAIN_DESC1 newDesc = new DXGI_SWAP_CHAIN_DESC1();
                        hr = m_pDXGISwapChain1.GetDesc1(out newDesc);
                        if (hr == HRESULT.S_OK)
                        {
                            System.Diagnostics.Debug.WriteLine($"[DEBUG] After resize: New swap chain dimensions: {newDesc.Width}x{newDesc.Height}");
                            if (newDesc.Width != sz.Width || newDesc.Height != sz.Height)
                            {
                                System.Diagnostics.Debug.WriteLine($"[DEBUG] WARNING: Actual swap chain size differs from requested size!");
                            }
                        }
                        
                        // Also notify the WinRT renderer about the size change
                        if (_isActive && _d2dRenderer != null)
                        {
                            try
                            {
                                System.Diagnostics.Debug.WriteLine($"[DEBUG] Notifying D2DRenderer of size change: {sz.Width}x{sz.Height}");
                                _d2dRenderer.Resize((uint)sz.Width, (uint)sz.Height);
                                System.Diagnostics.Debug.WriteLine($"[DEBUG] D2DRenderer resize completed successfully");
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
                        else
                        {
                            System.Diagnostics.Debug.WriteLine($"[DEBUG] Cannot resize D2DRenderer: _isActive={_isActive}, _d2dRenderer is {(_d2dRenderer == null ? "null" : "valid")}");
                        }
                    }
                    catch (Exception ex)
                    {
                        System.Diagnostics.Debug.WriteLine($"[DEBUG] Exception during ResizeBuffers: {ex.Message}");
                        hr = HRESULT.E_FAIL;
                    }
                }
                else
                {
                    System.Diagnostics.Debug.WriteLine($"[DEBUG] Skipping resize due to invalid dimensions: {sz.Width}x{sz.Height}");
                }
                
                // Reconfigure swap chain with new dimensions
                hr = ConfigureSwapChain();
                if (hr != HRESULT.S_OK)
                {
                    System.Diagnostics.Debug.WriteLine($"[DEBUG] ConfigureSwapChain failed after resize: 0x{hr:X}");
                }
            }
            else
            {
                System.Diagnostics.Debug.WriteLine("[DEBUG] Cannot resize: swap chain is null");
            }
            
            return hr;
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
                    
                    // Allow a moment for cleanup to complete
                    System.Threading.Thread.Sleep(50);
                }
                catch (Exception ex)
                {
                    System.Diagnostics.Debug.WriteLine($"Error releasing old swapchain: {ex.Message}");
                }
            }
            
            // Get panel dimensions if available - ALWAYS use the largest possible valid dimensions
            uint width = 1;
            uint height = 1;
            
            if (_swapChainPanel != null)
            {
                // Use actual dimensions or reasonable defaults if they're too small
                width = (uint)Math.Max(100, _swapChainPanel.ActualWidth);
                height = (uint)Math.Max(100, _swapChainPanel.ActualHeight);
                System.Diagnostics.Debug.WriteLine($"[DEBUG] Creating SwapChain with panel dimensions: {width}x{height}");
                
                // If for some reason the SwapChainPanel has zero dimensions,
                // try to use the parent window's dimensions if available
                if (width <= 1 || height <= 1)
                {
                    if (_hWndMain != IntPtr.Zero)
                    {
                        RECT rect = new RECT();
                        if (GetClientRect(_hWndMain, ref rect))
                        {
                            width = (uint)Math.Max(100, rect.right - rect.left);
                            height = (uint)Math.Max(100, rect.bottom - rect.top);
                            System.Diagnostics.Debug.WriteLine($"[DEBUG] Using window dimensions instead: {width}x{height}");
                        }
                    }
                    
                    // If we still don't have valid dimensions, use reasonable defaults
                    if (width <= 1 || height <= 1)
                    {
                        width = 800;
                        height = 600;
                        System.Diagnostics.Debug.WriteLine($"[DEBUG] Using default dimensions: {width}x{height}");
                    }
                }
            }
            else
            {
                // If no panel is available, use default dimensions
                width = 800;
                height = 600;
                System.Diagnostics.Debug.WriteLine($"[DEBUG] SwapChainPanel is null, using default size: {width}x{height}");
            }
            
            try
            {
                DXGI_SWAP_CHAIN_DESC1 swapChainDesc = new DXGI_SWAP_CHAIN_DESC1();
                swapChainDesc.Width = width;  // Never use 1x1, always use proper dimensions
                swapChainDesc.Height = height;
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

                        System.Diagnostics.Debug.WriteLine($"[DEBUG] SwapChain creation result: 0x{hr:X}, SwapChain is {(m_pDXGISwapChain1 != null ? "valid" : "null")}");
                        
                        // If successfully created, set frame latency
                        if (hr == HRESULT.S_OK && m_pDXGISwapChain1 != null)
                        {
                            hr = m_pDXGIDevice.SetMaximumFrameLatency(1);
                            
                            // Verify the created swap chain dimensions
                            DXGI_SWAP_CHAIN_DESC1 createdDesc = new DXGI_SWAP_CHAIN_DESC1();
                            HRESULT hr2 = m_pDXGISwapChain1.GetDesc1(out createdDesc);
                            if (hr2 == HRESULT.S_OK)
                            {
                                System.Diagnostics.Debug.WriteLine($"[DEBUG] Created SwapChain dimensions: {createdDesc.Width}x{createdDesc.Height}");
                                
                                if (createdDesc.Width != width || createdDesc.Height != height)
                                {
                                    System.Diagnostics.Debug.WriteLine($"[DEBUG] WARNING: Created swap chain dimensions differ from requested dimensions");
                                }
                            }
                        }
                        
                        SafeRelease(ref pDXGIFactory2);
                        Marshal.Release(pDXGIFactory2Ptr);
                    }
                    SafeRelease(ref pDXGIAdapter);
                }
            }
            catch (Exception ex)
            {
                System.Diagnostics.Debug.WriteLine($"[DEBUG] Exception creating swap chain: {ex.Message}");
                hr = HRESULT.E_FAIL;
            }
            
            return hr;
        }
        
        [StructLayout(LayoutKind.Sequential)]
        public struct RECT
        {
            public int left;
            public int top;
            public int right;
            public int bottom;
        }
        
        [DllImport("user32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool GetClientRect(IntPtr hWnd, ref RECT lpRect);

        public static HRESULT ConfigureSwapChain()
        {
            HRESULT hr = HRESULT.S_OK;
            
            try
            {
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
                
                // Get swap chain dimensions
                DXGI_SWAP_CHAIN_DESC1 swapDesc = new DXGI_SWAP_CHAIN_DESC1();
                hr = m_pDXGISwapChain1.GetDesc1(out swapDesc);
                if (hr == HRESULT.S_OK) 
                {
                    System.Diagnostics.Debug.WriteLine($"[DEBUG] Current SwapChain dimensions: {swapDesc.Width}x{swapDesc.Height}");
                    
                    // If the swap chain is still 1x1, and we have a valid panel with larger dimensions,
                    // we should resize it before configuring
                    if ((swapDesc.Width == 1 || swapDesc.Height == 1) && _swapChainPanel != null && 
                        _swapChainPanel.ActualWidth > 1 && _swapChainPanel.ActualHeight > 1)
                    {
                        System.Diagnostics.Debug.WriteLine($"[DEBUG] SwapChain is still 1x1 but panel is {_swapChainPanel.ActualWidth}x{_swapChainPanel.ActualHeight}");
                        System.Diagnostics.Debug.WriteLine($"[DEBUG] Resizing swap chain before configuring...");
                        
                        // First, we need to explicitly release all references to the swap chain buffers
                        // Make sure we don't have any references held by D2D
                        if (m_pD2DDeviceContext != null)
                        {
                            m_pD2DDeviceContext.SetTarget(null);
                        }
                        
                        if (m_pD2DTargetBitmap != null)
                        {
                            SafeRelease(ref m_pD2DTargetBitmap);
                        }
                        
                        // Force a present to flush any pending operations
                        try
                        {
                            m_pDXGISwapChain1.Present(0, DXGITools.DXGI_PRESENT_DO_NOT_WAIT);
                        }
                        catch (Exception ex)
                        {
                            System.Diagnostics.Debug.WriteLine($"[DEBUG] Non-critical error during buffer flush: {ex.Message}");
                        }
                        
                        // Force GC to clean up any references that might be held by the runtime
                        GC.Collect();
                        GC.WaitForPendingFinalizers();
                        
                        // Wait a small amount of time to ensure GPU operations complete
                        System.Threading.Thread.Sleep(50);
                        
                        hr = m_pDXGISwapChain1.ResizeBuffers(
                            2,
                            (uint)_swapChainPanel.ActualWidth,
                            (uint)_swapChainPanel.ActualHeight,
                            DXGI_FORMAT.DXGI_FORMAT_B8G8R8A8_UNORM,
                            0
                        );
                        
                        if (hr != HRESULT.S_OK) {
                            System.Diagnostics.Debug.WriteLine($"[DEBUG] Failed to resize swap chain: 0x{hr:X}");
                            
                            if ((uint)hr == 0x887A0001) // DXGI_ERROR_INVALID_CALL
                            {
                                System.Diagnostics.Debug.WriteLine("[DEBUG] Attempting to recreate swap chain due to buffer reference issue");
                                
                                // If we still have buffer references, we need more aggressive cleanup
                                SafeRelease(ref m_pD2DTargetBitmap);
                                
                                if (m_pDXGISwapChain1 != null)
                                {
                                    SafeRelease(ref m_pDXGISwapChain1);
                                }
                                
                                // Force another GC cycle
                                GC.Collect();
                                GC.WaitForPendingFinalizers();
                                
                                // Create a new swap chain with the correct dimensions
                                hr = CreateSwapChain(IntPtr.Zero);
                                if (hr != HRESULT.S_OK)
                                {
                                    System.Diagnostics.Debug.WriteLine($"[DEBUG] Failed to recreate swap chain: 0x{hr:X}");
                                    return hr;
                                }
                                else
                                {
                                    System.Diagnostics.Debug.WriteLine("[DEBUG] Successfully recreated swap chain");
                                }
                            }
                        } else {
                            System.Diagnostics.Debug.WriteLine($"[DEBUG] Successfully resized swap chain to {_swapChainPanel.ActualWidth}x{_swapChainPanel.ActualHeight}");
                            
                            // Update our local copy of the desc
                            hr = m_pDXGISwapChain1.GetDesc1(out swapDesc);
                            if (hr == HRESULT.S_OK) {
                                System.Diagnostics.Debug.WriteLine($"[DEBUG] Updated SwapChain dimensions: {swapDesc.Width}x{swapDesc.Height}");
                            }
                        }
                    }
                }
                
                // Wait for GPU to finish all operations before trying to access the swap chain buffer
                System.Threading.Thread.Sleep(16);
                
                D2D1_BITMAP_PROPERTIES1 bitmapProperties = new D2D1_BITMAP_PROPERTIES1();
                bitmapProperties.bitmapOptions = D2D1_BITMAP_OPTIONS.D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS.D2D1_BITMAP_OPTIONS_CANNOT_DRAW;
                bitmapProperties.pixelFormat = D2DTools.PixelFormat(DXGI_FORMAT.DXGI_FORMAT_B8G8R8A8_UNORM, D2D1_ALPHA_MODE.D2D1_ALPHA_MODE_PREMULTIPLIED);
                uint nDPI = GetDpiForWindow(_hWndMain);
                if (nDPI == 0) nDPI = 96; // Use default DPI if window handle is invalid
                bitmapProperties.dpiX = nDPI;
                bitmapProperties.dpiY = nDPI;
                
                System.Diagnostics.Debug.WriteLine($"[DEBUG] Using DPI: {nDPI}x{nDPI}");

                IntPtr pDXGISurfacePtr = IntPtr.Zero;
                hr = m_pDXGISwapChain1.GetBuffer(0, typeof(IDXGISurface).GUID, out pDXGISurfacePtr);
                
                if (hr == HRESULT.S_OK && pDXGISurfacePtr != IntPtr.Zero)
                {
                    System.Diagnostics.Debug.WriteLine("Successfully acquired swap chain buffer");
                    
                    IDXGISurface pDXGISurface = Marshal.GetObjectForIUnknown(pDXGISurfacePtr) as IDXGISurface;
                    
                    if (pDXGISurface != null)
                    {
                        // Get the surface size to verify dimensions
                        DXGI_SURFACE_DESC surfaceDesc = new DXGI_SURFACE_DESC();
                        hr = pDXGISurface.GetDesc(out surfaceDesc);
                        
                        if (hr == HRESULT.S_OK) {
                            System.Diagnostics.Debug.WriteLine($"[DEBUG] Surface dimensions: {surfaceDesc.Width}x{surfaceDesc.Height}, Format: {surfaceDesc.Format}");
                        }
                        
                        hr = m_pD2DDeviceContext.CreateBitmapFromDxgiSurface(pDXGISurface, ref bitmapProperties, out m_pD2DTargetBitmap);
                        
                        if (hr == HRESULT.S_OK && m_pD2DTargetBitmap != null)
                        {
                            System.Diagnostics.Debug.WriteLine("Successfully created target bitmap from DXGI surface");
                            
                            D2D1_SIZE_F bitmapSize = m_pD2DTargetBitmap.GetSize();
                            System.Diagnostics.Debug.WriteLine($"[DEBUG] D2D Target Bitmap size: {bitmapSize.width}x{bitmapSize.height}");
                            
                            m_pD2DDeviceContext.SetTarget(m_pD2DTargetBitmap);
                            
                            // IMPORTANT: We're no longer clearing with white here
                            // Let the Rust-based renderer control the background color
                            
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
                            
                            // If bitmap creation failed but we have the surface, try to clear the surface directly
                            if (surfaceDesc.Width <= 1 || surfaceDesc.Height <= 1)
                            {
                                System.Diagnostics.Debug.WriteLine("[DEBUG] Surface dimensions invalid, attempting recovery");
                                
                                // Surface could be valid but bitmap creation failed, possibly due to zero dimensions
                                if (_swapChainPanel != null && _swapChainPanel.ActualWidth > 1 && _swapChainPanel.ActualHeight > 1)
                                {
                                    SafeRelease(ref m_pDXGISwapChain1);
                                    
                                    // Recreate with explicit dimensions from the panel
                                    hr = CreateSwapChain(IntPtr.Zero);
                                    
                                    if (hr == HRESULT.S_OK)
                                    {
                                        System.Diagnostics.Debug.WriteLine("[DEBUG] Successfully recreated swap chain after bitmap failure");
                                        // Don't continue further in this call, we'll configure in the next frame
                                    }
                                }
                            }
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
