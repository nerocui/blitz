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

        // Performance monitoring variables
        static private long _lastPerformanceCheckTime = 0;
        static private int _frameCount = 0;
        static private float _currentFps = 0;
        static private bool _showPerformanceOverlay = false; // Will be controlled by UI toggle
        static private readonly int _fpsUpdateInterval = 500; // Update FPS display every 500ms
        static private Dictionary<string, PerformanceMetric> _performanceMetrics = new Dictionary<string, PerformanceMetric>();

        public static void Initialize(IntPtr hWndMain)
        {
            _hWndMain = hWndMain;
            Microsoft.UI.WindowId myWndId = Microsoft.UI.Win32Interop.GetWindowIdFromWindow(hWndMain);
            _apw = Microsoft.UI.Windowing.AppWindow.GetFromWindowId(myWndId);

            m_pWICImagingFactory = (IWICImagingFactory)Activator.CreateInstance(Type.GetTypeFromCLSID(WICTools.CLSID_WICImagingFactory));

            liFreq = new LARGE_INTEGER();
            QueryPerformanceFrequency(out liFreq);

            // Initialize performance metrics
            InitializePerformanceMonitoring();

            HRESULT hr = CreateD2D1Factory();
            if (hr == HRESULT.S_OK)
            {
                hr = CreateDeviceContext();
            }
        }

        private static void InitializePerformanceMonitoring()
        {
            // Initialize performance tracking
            _lastPerformanceCheckTime = GetHighPrecisionTimestamp();
            _frameCount = 0;
            _currentFps = 0;
            
            // Create metrics for key operations
            _performanceMetrics.Clear();
            _performanceMetrics["Render"] = new PerformanceMetric("Render");
            _performanceMetrics["Present"] = new PerformanceMetric("Present");
            _performanceMetrics["Tick"] = new PerformanceMetric("Tick");
            _performanceMetrics["Total"] = new PerformanceMetric("Total");
            
            System.Diagnostics.Debug.WriteLine("Performance monitoring initialized");
        }
        
        private static long GetHighPrecisionTimestamp()
        {
            LARGE_INTEGER time = new LARGE_INTEGER();
            QueryPerformanceCounter(out time);
            return time.QuadPart;
        }
        
        private static float ConvertToMilliseconds(long start, long end)
        {
            return (float)((end - start) * 1000.0 / liFreq.QuadPart);
        }
        
        private static void UpdatePerformanceStatistics()
        {
            long currentTime = GetHighPrecisionTimestamp();
            _frameCount++;
            
            // Update FPS counter every update interval
            float timeDelta = ConvertToMilliseconds(_lastPerformanceCheckTime, currentTime);
            if (timeDelta >= _fpsUpdateInterval)
            {
                _currentFps = (float)(_frameCount * 1000.0 / timeDelta);
                _lastPerformanceCheckTime = currentTime;
                _frameCount = 0;
                
                // Log performance metrics periodically
                string metricsLog = $"FPS: {_currentFps:F1}";
                foreach (var metric in _performanceMetrics.Values)
                {
                    metricsLog += $", {metric.Name}: {metric.AverageTimeMs:F2}ms";
                    metric.ResetAverages(); // Reset after logging
                }
                System.Diagnostics.Debug.WriteLine($"[PERF] {metricsLog}");
            }
        }
        
        private static void BeginTimeMeasure(string metricName)
        {
            if (_performanceMetrics.TryGetValue(metricName, out PerformanceMetric metric))
            {
                metric.Begin = GetHighPrecisionTimestamp();
            }
        }
        
        private static void EndTimeMeasure(string metricName)
        {
            if (_performanceMetrics.TryGetValue(metricName, out PerformanceMetric metric))
            {
                metric.End = GetHighPrecisionTimestamp();
                metric.UpdateAverage(ConvertToMilliseconds(metric.Begin, metric.End));
            }
        }

        // Helper class for tracking performance metrics
        private class PerformanceMetric
        {
            public string Name { get; }
            public long Begin { get; set; }
            public long End { get; set; }
            public float AverageTimeMs { get; private set; }
            public float MinTimeMs { get; private set; }
            public float MaxTimeMs { get; private set; }
            private int _sampleCount;
            private float _totalTimeMs;
            
            public PerformanceMetric(string name)
            {
                Name = name;
                ResetAverages();
            }
            
            public void UpdateAverage(float timeMs)
            {
                _totalTimeMs += timeMs;
                _sampleCount++;
                
                if (timeMs < MinTimeMs || MinTimeMs == 0)
                    MinTimeMs = timeMs;
                
                if (timeMs > MaxTimeMs)
                    MaxTimeMs = timeMs;
                
                AverageTimeMs = _totalTimeMs / _sampleCount;
            }
            
            public void ResetAverages()
            {
                _totalTimeMs = 0;
                _sampleCount = 0;
                AverageTimeMs = 0;
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
            
            // Initialize properly with valid dimensions before creating the swap chain
            double initialWidth = 800;  // Default width
            double initialHeight = 600; // Default height
            
            // Try to get valid dimensions before proceeding
            if (_swapChainPanel != null)
            {
                // Try to determine the actual size of the panel
                // Check ActualWidth and ActualHeight first
                if (!double.IsNaN(_swapChainPanel.ActualWidth) && !double.IsNaN(_swapChainPanel.ActualHeight) &&
                    _swapChainPanel.ActualWidth > 0 && _swapChainPanel.ActualHeight > 0)
                {
                    initialWidth = _swapChainPanel.ActualWidth;
                    initialHeight = _swapChainPanel.ActualHeight;
                    System.Diagnostics.Debug.WriteLine($"Using panel's ActualSize: {initialWidth}x{initialHeight}");
                }
                // Then try Width and Height if ActualWidth/Height are not valid
                else if (!double.IsNaN(_swapChainPanel.Width) && !double.IsNaN(_swapChainPanel.Height) &&
                         _swapChainPanel.Width > 0 && _swapChainPanel.Height > 0)
                {
                    initialWidth = _swapChainPanel.Width;
                    initialHeight = _swapChainPanel.Height;
                    System.Diagnostics.Debug.WriteLine($"Using panel's requested size: {initialWidth}x{initialHeight}");
                }
                // Try to get parent container size
                else
                {
                    var parent = VisualTreeHelper.GetParent(swapChainPanel) as FrameworkElement;
                    if (parent != null && 
                        !double.IsNaN(parent.ActualWidth) && !double.IsNaN(parent.ActualHeight) &&
                        parent.ActualWidth > 0 && parent.ActualHeight > 0)
                    {
                        initialWidth = parent.ActualWidth;
                        initialHeight = parent.ActualHeight;
                        System.Diagnostics.Debug.WriteLine($"Using parent container size: {initialWidth}x{initialHeight}");
                    }
                    else
                    {
                        System.Diagnostics.Debug.WriteLine($"No valid size found, using default: {initialWidth}x{initialHeight}");
                    }
                }
                
                // Attach size changed event before creating swap chain
                _swapChainPanel.SizeChanged += scpD2D_SizeChanged;
                
                // Also set up a loaded event to capture initial size if needed
                _swapChainPanel.Loaded += (sender, e) => {
                    System.Diagnostics.Debug.WriteLine($"SwapChainPanel Loaded event: ActualSize={_swapChainPanel.ActualWidth}x{_swapChainPanel.ActualHeight}");
                    
                    // If we now have valid dimensions and they're different from what we started with,
                    // resize to the new dimensions
                    if (!double.IsNaN(_swapChainPanel.ActualWidth) && !double.IsNaN(_swapChainPanel.ActualHeight) &&
                        _swapChainPanel.ActualWidth > 0 && _swapChainPanel.ActualHeight > 0 &&
                        (Math.Abs(_swapChainPanel.ActualWidth - initialWidth) > 1 || 
                         Math.Abs(_swapChainPanel.ActualHeight - initialHeight) > 1))
                    {
                        System.Diagnostics.Debug.WriteLine($"Resizing after load to: {_swapChainPanel.ActualWidth}x{_swapChainPanel.ActualHeight}");
                        Resize(new Size(_swapChainPanel.ActualWidth, _swapChainPanel.ActualHeight));
                    }
                };
            }
            
            // Now create the swap chain with our validated dimensions
            var hr = CreateSwapChain(IntPtr.Zero, (uint)initialWidth, (uint)initialHeight);
            if (hr == HRESULT.S_OK)
            {
                hr = ConfigureSwapChain();
                ISwapChainPanelNative panelNative = WinRT.CastExtensions.As<ISwapChainPanelNative>(swapChainPanel);
                hr = panelNative.SetSwapChain(m_pDXGISwapChain1);
            }
            
            // Set up the composition rendering
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
                    
                    // Set the initial size based on the verified dimensions
                    if (_isActive && _d2dRenderer != null)
                    {
                        // Ensure size is positive and valid
                        System.Diagnostics.Debug.WriteLine($"Resizing to {initialWidth}x{initialHeight}");
                        
                        // CRITICAL: Wait a short time to ensure the swap chain and target are fully configured
                        System.Threading.Thread.Sleep(50);
                        
                        // Check if target is ready before resizing
                        bool targetReady = false;
                        try
                        {
                            ID2D1Image currentTarget = null;
                            if (m_pD2DDeviceContext != null)
                            {
                                m_pD2DDeviceContext.GetTarget(out currentTarget);
                                targetReady = (currentTarget != null);
                                if (currentTarget != null)
                                    SafeRelease(ref currentTarget);
                            }
                        }
                        catch (Exception ex)
                        {
                            System.Diagnostics.Debug.WriteLine($"Error checking target: {ex.Message}");
                        }
                        
                        if (!targetReady)
                        {
                            System.Diagnostics.Debug.WriteLine("Target not ready, reconfiguring swap chain");
                            hr = ConfigureSwapChain();
                            System.Threading.Thread.Sleep(50);
                        }
                        
                        // Now resize the renderer with our validated dimensions
                        _d2dRenderer.Resize((uint)initialWidth, (uint)initialHeight);
                        
                        // Wait a short time before doing the initial render
                        System.Threading.Thread.Sleep(50);
                        
                        // CRITICAL FIX: Check if target is valid before rendering
                        bool canRender = false;
                        try
                        {
                            ID2D1Image currentTarget = null;
                            if (m_pD2DDeviceContext != null)
                            {
                                m_pD2DDeviceContext.GetTarget(out currentTarget);
                                canRender = (currentTarget != null);
                                if (currentTarget != null)
                                    SafeRelease(ref currentTarget);
                            }
                        }
                        catch (Exception ex)
                        {
                            System.Diagnostics.Debug.WriteLine($"Error checking target before render: {ex.Message}");
                        }
                        
                        // Only attempt initial render if target is valid
                        if (canRender && _markdown != null)
                        {
                            System.Diagnostics.Debug.WriteLine($"Explicitly calling Render with markdown content length: {_markdown.Length}");
                            Render();
                        }
                        else
                        {
                            System.Diagnostics.Debug.WriteLine("Skipping initial render due to NULL target - will render in next frame");
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

        public static HRESULT CreateSwapChain(IntPtr hWnd, uint requestedWidth = 0, uint requestedHeight = 0)
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
            uint width = 800;  // Default
            uint height = 600; // Default
            
            // If explicit dimensions are provided, use them
            if (requestedWidth > 0 && requestedHeight > 0)
            {
                width = requestedWidth;
                height = requestedHeight;
                System.Diagnostics.Debug.WriteLine($"[DEBUG] Creating SwapChain with explicit dimensions: {width}x{height}");
            }
            else if (_swapChainPanel != null)
            {
                // Use actual dimensions or reasonable defaults if they're too small
                // Make sure to check for NaN before using values
                if (!double.IsNaN(_swapChainPanel.ActualWidth) && !double.IsNaN(_swapChainPanel.ActualHeight) &&
                    _swapChainPanel.ActualWidth > 0 && _swapChainPanel.ActualHeight > 0)
                {
                    width = (uint)Math.Max(100, _swapChainPanel.ActualWidth);
                    height = (uint)Math.Max(100, _swapChainPanel.ActualHeight);
                    System.Diagnostics.Debug.WriteLine($"[DEBUG] Creating SwapChain with panel dimensions: {width}x{height}");
                }
                // If for some reason the SwapChainPanel has zero dimensions,
                // try to use the parent window's dimensions if available
                else if (_hWndMain != IntPtr.Zero)
                {
                    RECT rect = new RECT();
                    if (GetClientRect(_hWndMain, ref rect))
                    {
                        width = (uint)Math.Max(100, rect.right - rect.left);
                        height = (uint)Math.Max(100, rect.bottom - rect.top);
                        System.Diagnostics.Debug.WriteLine($"[DEBUG] Using window dimensions instead: {width}x{height}");
                    }
                }
                else
                {
                    System.Diagnostics.Debug.WriteLine($"[DEBUG] Using default dimensions: {width}x{height}");
                }
            }
            else
            {
                // If no panel is available, use default dimensions
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

        // Public method to toggle performance overlay visibility
        public static void TogglePerformanceOverlay()
        {
            _showPerformanceOverlay = !_showPerformanceOverlay;
            System.Diagnostics.Debug.WriteLine($"Performance overlay {(_showPerformanceOverlay ? "enabled" : "disabled")}");
        }
        
        // Method to get performance data as a formatted string for external display
        public static string GetPerformanceData()
        {
            string result = $"FPS: {_currentFps:F1}\n";
            
            foreach (var metric in _performanceMetrics.Values)
            {
                result += $"{metric.Name}: {metric.AverageTimeMs:F2}ms (Min: {metric.MinTimeMs:F2}ms, Max: {metric.MaxTimeMs:F2}ms)\n";
            }
            
            return result;
        }

        [DllImport("Kernel32.dll", SetLastError = true, CharSet = CharSet.Auto)]
        public static extern bool QueryPerformanceCounter(out LARGE_INTEGER lpPerformanceCount);

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
