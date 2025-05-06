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
using Microsoft.UI.Dispatching; // Add this for DispatcherQueue and DispatcherQueuePriority
using MarkdownTest.Logging;

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

        // Frame dropping mechanism variables
        static private bool _isRenderingFrame = false;
        static private long _renderingStartTime = 0;
        static private int _droppedFrameCount = 0;
        static private int _consecutiveDroppedFrames = 0;
        static private int _totalFrames = 0;
        static private readonly float _targetFrameTimeMs = 16.66f; // Target frame time for 60fps
        static private readonly int _maxConsecutiveDroppedFrames = 5; // Force render after dropping this many consecutive frames
        
        // Throttling mechanism to limit render calls to 60fps
        static private long _lastFrameTimestamp = 0;
        static private readonly float _minimumFrameTimeMs = 16.66f; // Don't render faster than 60fps (1000/60)
        
        // Resize debouncing to prevent excessive reconfiguration
        static private DateTime _lastResizeTime = DateTime.MinValue;
        static private bool _resizePending = false;
        static private Size _pendingResizeSize = new Size(0, 0);
        static private readonly TimeSpan _resizeDebounceTime = TimeSpan.FromMilliseconds(200); // Wait 200ms between resize operations
        static private DispatcherQueueTimer _resizeTimer = null;

        // Circuit breaker variables
        private static int _targetReconfigurationAttempts = 0;
        private static readonly int _maxReconfigurationAttempts = 3;
        private static DateTime _lastReconfigurationTime = DateTime.MinValue;
        private static TimeSpan _reconfigurationCooldown = TimeSpan.FromSeconds(5);

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
            // Log the size change event for debugging
            System.Diagnostics.Debug.WriteLine($"[DEBUG] Panel size changed: {e.NewSize.Width}x{e.NewSize.Height}");
            
            // Check if the new size is valid
            if (e.NewSize.Width > 0 && e.NewSize.Height > 0)
            {
                // Update pending resize size
                _pendingResizeSize = e.NewSize;
                _resizePending = true;
                
                // Get the DispatcherQueue from the sender object (SwapChainPanel)
                var panel = sender as FrameworkElement;
                if (panel != null)
                {
                    // Create the timer if it doesn't exist
                    if (_resizeTimer == null)
                    {
                        _resizeTimer = panel.DispatcherQueue.CreateTimer();
                        _resizeTimer.Interval = TimeSpan.FromMilliseconds(200); // 200ms debounce
                        _resizeTimer.Tick += (s, args) =>
                        {
                            _resizeTimer.Stop();
                            
                            if (_resizePending)
                            {
                                System.Diagnostics.Debug.WriteLine($"[DEBUG] Debounced resize timer triggered: {_pendingResizeSize.Width}x{_pendingResizeSize.Height}");
                                ResizeWithReducedFlashing(_pendingResizeSize);
                                _resizePending = false;
                            }
                        };
                    }
                    
                    // Restart the timer
                    _resizeTimer.Stop();
                    _resizeTimer.Start();
                    
                    // Track when we last received a resize event
                    _lastResizeTime = DateTime.Now;
                }
                else
                {
                    // If we can't get the dispatcher queue, resize directly
                    System.Diagnostics.Debug.WriteLine($"[DEBUG] No dispatcher available, resizing directly to: {e.NewSize.Width}x{e.NewSize.Height}");
                    ResizeWithReducedFlashing(e.NewSize);
                }
            }
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

        public static void SetupRendering(SwapChainPanel swapChainPanel, string markdown)
        {
            System.Diagnostics.Debug.WriteLine($"Setting up rendering with markdown content length: {markdown?.Length}");
            
            // First ensure a proper cleanup to avoid resource conflicts
            UnloadPage();
            
            // CRITICAL FIX: Add more time for resource cleanup
            // System.Threading.Thread.Sleep(100);
            
            // Force garbage collection before creating new resources
            GC.Collect();
            GC.WaitForPendingFinalizers();
            
            _rendered = false;
            _swapChainPanel = swapChainPanel;
            _markdown = markdown;
            
            // Initialize properly with valid dimensions before creating the swap chain
            double initialWidth = 800;  // Default width
            double initialHeight = 600; // Default height
            
            // Get the actual size from the SwapChainPanel or its parent container
            if (_swapChainPanel != null)
            {
                // Try to determine the actual size of the panel
                if (_swapChainPanel.ActualWidth > 0 && _swapChainPanel.ActualHeight > 0)
                {
                    initialWidth = _swapChainPanel.ActualWidth;
                    initialHeight = _swapChainPanel.ActualHeight;
                    System.Diagnostics.Debug.WriteLine($"Using panel's ActualSize: {initialWidth}x{initialHeight}");
                }
                else if (_swapChainPanel.Parent is FrameworkElement parent)
                {
                    // Try to get size from parent
                    if (parent.ActualWidth > 0 && parent.ActualHeight > 0)
                    {
                        initialWidth = parent.ActualWidth;
                        initialHeight = parent.ActualHeight;
                        System.Diagnostics.Debug.WriteLine($"Using parent container size: {initialWidth}x{initialHeight}");
                    }
                }
                
                // Ensure size is at least our minimum defaults
                initialWidth = Math.Max(initialWidth, 800);
                initialHeight = Math.Max(initialHeight, 600);
                System.Diagnostics.Debug.WriteLine($"Final initial dimensions: {initialWidth}x{initialHeight}");
                
                // Attach size changed event before creating swap chain
                _swapChainPanel.SizeChanged += scpD2D_SizeChanged;
                
                // Also set up a loaded event to capture initial size if needed
                _swapChainPanel.Loaded += (sender, e) => {
                    System.Diagnostics.Debug.WriteLine($"SwapChainPanel Loaded event: ActualSize={_swapChainPanel.ActualWidth}x{_swapChainPanel.ActualHeight}");
                    
                    // If we now have valid dimensions and they're different from what we started with,
                    // resize to the new dimensions
                    if (_swapChainPanel.ActualWidth > 0 && _swapChainPanel.ActualHeight > 0 &&
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
                // CRITICAL FIX: Add delay before configuring swap chain
                // System.Threading.Thread.Sleep(50);
                
                hr = ConfigureSwapChain();
                
                if (hr == HRESULT.S_OK)
                {
                    // CRITICAL FIX: Verify the swap chain target is properly set
                    bool targetIsValid = false;
                    
                    if (m_pD2DDeviceContext != null)
                    {
                        try
                        {
                            ID2D1Image verifyTarget = null;
                            m_pD2DDeviceContext.GetTarget(out verifyTarget);
                            
                            if (verifyTarget != null)
                            {
                                targetIsValid = true;
                                System.Diagnostics.Debug.WriteLine("[DEBUG] Initial target is valid after configuration");
                                SafeRelease(ref verifyTarget);
                            }
                            else
                            {
                                System.Diagnostics.Debug.WriteLine("[DEBUG] Initial target is NULL after configuration!");
                                
                                // If target bitmap exists but isn't set as the target, set it
                                if (m_pD2DTargetBitmap != null)
                                {
                                    System.Diagnostics.Debug.WriteLine("[DEBUG] Attempting to set target explicitly");
                                    m_pD2DDeviceContext.SetTarget(m_pD2DTargetBitmap);
                                    
                                    // Verify again
                                    m_pD2DDeviceContext.GetTarget(out verifyTarget);
                                    if (verifyTarget != null)
                                    {
                                        targetIsValid = true;
                                        System.Diagnostics.Debug.WriteLine("[DEBUG] Target is now valid after explicit set");
                                        SafeRelease(ref verifyTarget);
                                    }
                                    else
                                    {
                                        System.Diagnostics.Debug.WriteLine("[DEBUG] Target is still NULL after explicit set!");
                                    }
                                }
                            }
                        }
                        catch (Exception ex)
                        {
                            System.Diagnostics.Debug.WriteLine($"[DEBUG] Error verifying target: {ex.Message}");
                        }
                    }
                    
                    // CRITICAL FIX: If target validation failed, try to reconfigure one more time
                    if (!targetIsValid && m_pDXGISwapChain1 != null)
                    {
                        System.Diagnostics.Debug.WriteLine("[DEBUG] Target validation failed, attempting to reconfigure");
                        
                        // Reset everything and reconfigure
                        if (m_pD2DDeviceContext != null)
                        {
                            try { m_pD2DDeviceContext.SetTarget(null); } catch { }
                        }
                        
                        SafeRelease(ref m_pD2DTargetBitmap);
                        // System.Threading.Thread.Sleep(100);
                        
                        hr = ConfigureSwapChain();
                        System.Diagnostics.Debug.WriteLine($"[DEBUG] Reconfiguration result: 0x{hr:X}");
                    }
                }
                
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
                        _d2dLogger = new D2DLogger();
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
                        System.Diagnostics.Debug.WriteLine($"Setting initial renderer size to {initialWidth}x{initialHeight}");
                        
                        // CRITICAL FIX: Wait longer to ensure the swap chain and target are fully configured
                        // System.Threading.Thread.Sleep(100);
                        
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
                            // System.Threading.Thread.Sleep(100);
                            
                            // Verify target again
                            try
                            {
                                ID2D1Image recheckTarget = null;
                                if (m_pD2DDeviceContext != null)
                                {
                                    m_pD2DDeviceContext.GetTarget(out recheckTarget);
                                    targetReady = (recheckTarget != null);
                                    if (recheckTarget != null)
                                        SafeRelease(ref recheckTarget);
                                }
                            }
                            catch (Exception ex)
                            {
                                System.Diagnostics.Debug.WriteLine($"Error re-checking target: {ex.Message}");
                            }
                        }
                        
                        // CRITICAL FIX: Ensure target is valid before proceeding to resize
                        if (targetReady)
                        {
                            // Now resize the renderer with our validated dimensions
                            try
                            {
                                _d2dRenderer.Resize((uint)initialWidth, (uint)initialHeight);
                                System.Diagnostics.Debug.WriteLine("[DEBUG] D2DRenderer resized successfully");
                            }
                            catch (Exception ex)
                            {
                                System.Diagnostics.Debug.WriteLine($"[DEBUG] Error during renderer resize: {ex.Message}");
                            }
                            
                            // Wait a short time before doing the initial render
                            // System.Threading.Thread.Sleep(100);
                            
                            // CRITICAL FIX: Explicitly call Render immediately after setup to initialize content
                            if (_markdown != null)
                            {
                                System.Diagnostics.Debug.WriteLine($"Explicitly calling Render with markdown content length: {_markdown.Length}");
                                Render();
                            }
                        }
                        else
                        {
                            System.Diagnostics.Debug.WriteLine("[DEBUG] Target still not ready after reconfiguration");
                            
                            // Last resort - recreate the entire pipeline
                            SafeRelease(ref m_pD2DTargetBitmap);
                            if (m_pDXGISwapChain1 != null)
                            {
                                SafeRelease(ref m_pDXGISwapChain1);
                            }
                            
                            // System.Threading.Thread.Sleep(100);
                            GC.Collect();
                            GC.WaitForPendingFinalizers();
                            
                            hr = CreateSwapChain(IntPtr.Zero, (uint)initialWidth, (uint)initialHeight);
                            if (hr == HRESULT.S_OK)
                            {
                                hr = ConfigureSwapChain();
                                if (hr == HRESULT.S_OK)
                                {
                                    // Now try to set size and render
                                    try
                                    {
                                        _d2dRenderer.Resize((uint)initialWidth, (uint)initialHeight);
                                        
                                        if (_markdown != null)
                                        {
                                            System.Diagnostics.Debug.WriteLine($"Trying render after full recreation");
                                            Render();
                                        }
                                    }
                                    catch (Exception ex)
                                    {
                                        System.Diagnostics.Debug.WriteLine($"[DEBUG] Error during renderer resize/render after recreation: {ex.Message}");
                                    }
                                }
                            }
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
                    // System.Threading.Thread.Sleep(50);
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

        public static HRESULT CreateSwapChain(IntPtr hWnd)
        {
            // This is a compatibility method to match the signature from the old file
            // Default to 0 dimensions which will cause the method to determine dimensions from the panel
            return CreateSwapChain(hWnd, 0, 0);
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
                    if ((swapDesc.Width <= 1 || swapDesc.Height <= 1) && _swapChainPanel != null && 
                        _swapChainPanel.ActualWidth > 1 && _swapChainPanel.ActualHeight > 1)
                    {
                        System.Diagnostics.Debug.WriteLine($"[DEBUG] SwapChain is too small but panel is {_swapChainPanel.ActualWidth}x{_swapChainPanel.ActualHeight}");
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
                        // System.Threading.Thread.Sleep(50);
                        
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
                // System.Threading.Thread.Sleep(16);
                
                // Setup bitmap properties
                D2D1_BITMAP_PROPERTIES1 bitmapProperties = new D2D1_BITMAP_PROPERTIES1();
                bitmapProperties.bitmapOptions = D2D1_BITMAP_OPTIONS.D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS.D2D1_BITMAP_OPTIONS_CANNOT_DRAW;
                bitmapProperties.pixelFormat = D2DTools.PixelFormat(DXGI_FORMAT.DXGI_FORMAT_B8G8R8A8_UNORM, D2D1_ALPHA_MODE.D2D1_ALPHA_MODE_PREMULTIPLIED);
                uint nDPI = GetDpiForWindow(_hWndMain);
                if (nDPI == 0) nDPI = 96; // Use default DPI if window handle is invalid
                bitmapProperties.dpiX = nDPI;
                bitmapProperties.dpiY = nDPI;
                
                System.Diagnostics.Debug.WriteLine($"[DEBUG] Using DPI: {nDPI}x{nDPI}");

                // Get the buffer surface
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
                        
                        // Create bitmap from DXGI surface
                        try
                        {
                            hr = m_pD2DDeviceContext.CreateBitmapFromDxgiSurface(pDXGISurface, ref bitmapProperties, out m_pD2DTargetBitmap);
                            
                            if (hr == HRESULT.S_OK && m_pD2DTargetBitmap != null)
                            {
                                System.Diagnostics.Debug.WriteLine("Successfully created target bitmap from DXGI surface");
                                
                                D2D1_SIZE_F bitmapSize = m_pD2DTargetBitmap.GetSize();
                                System.Diagnostics.Debug.WriteLine($"[DEBUG] D2D Target Bitmap size: {bitmapSize.width}x{bitmapSize.height}");
                                
                                // CRITICAL FIX: Make sure we set the target on the device context
                                m_pD2DDeviceContext.SetTarget(m_pD2DTargetBitmap);

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
                                
                                // If bitmap creation failed but we have the surface, try to recover
                                if (surfaceDesc.Width <= 1 || surfaceDesc.Height <= 1)
                                {
                                    System.Diagnostics.Debug.WriteLine("[DEBUG] Surface dimensions invalid, attempting recovery");
                                    
                                    // Try to recreate with explicit dimensions
                                    if (_swapChainPanel != null && _swapChainPanel.ActualWidth > 1 && _swapChainPanel.ActualHeight > 1)
                                    {
                                        SafeRelease(ref m_pDXGISwapChain1);
                                        
                                        // Recreate with explicit dimensions from the panel
                                        double width = Math.Max(800, _swapChainPanel.ActualWidth);
                                        double height = Math.Max(600, _swapChainPanel.ActualHeight);
                                        hr = CreateSwapChain(IntPtr.Zero, (uint)width, (uint)height);
                                        
                                        if (hr == HRESULT.S_OK)
                                        {
                                            System.Diagnostics.Debug.WriteLine("[DEBUG] Successfully recreated swap chain after bitmap failure");
                                        }
                                    }
                                }
                            }
                        }
                        catch (Exception ex)
                        {
                            System.Diagnostics.Debug.WriteLine($"Exception creating bitmap from surface: {ex.Message} (0x{ex.HResult:X})");
                            hr = HRESULT.E_FAIL;
                            
                            // Recovery attempt
                            if (m_pDXGISwapChain1 != null && _swapChainPanel != null)
                            {
                                SafeRelease(ref m_pDXGISwapChain1);
                                
                                // Force GC cycle
                                GC.Collect();
                                GC.WaitForPendingFinalizers();
                                
                                // Recreate with explicit dimensions
                                double width = Math.Max(800, _swapChainPanel.ActualWidth);
                                double height = Math.Max(600, _swapChainPanel.ActualHeight);
                                hr = CreateSwapChain(IntPtr.Zero, (uint)width, (uint)height);
                                
                                if (hr == HRESULT.S_OK)
                                {
                                    System.Diagnostics.Debug.WriteLine("[DEBUG] Successfully recreated swap chain after exception");
                                    // Call ConfigureSwapChain again, but make sure we don't recurse infinitely
                                    // by returning immediately
                                    return hr;
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

                // Start performance tracking for resize operation
                BeginTimeMeasure("Resize");

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
                    // System.Threading.Thread.Sleep(50);
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
                            // System.Threading.Thread.Sleep(100);
                            
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

                // End performance tracking for resize operation
                EndTimeMeasure("Resize");
            }
            else
            {
                System.Diagnostics.Debug.WriteLine("[DEBUG] Cannot resize: swap chain is null");
            }
            
            return hr;
        }

        /// <summary>
        /// Special resize method that minimizes white flashing during resize operations
        /// </summary>
        public static HRESULT ResizeWithReducedFlashing(Size sz)
        {
            HRESULT hr = HRESULT.S_OK;

            // Check if we're already at this size
            if (m_pDXGISwapChain1 != null)
            {
                DXGI_SWAP_CHAIN_DESC1 origDesc = new DXGI_SWAP_CHAIN_DESC1();
                try
                {
                    hr = m_pDXGISwapChain1.GetDesc1(out origDesc);
                    if (hr == HRESULT.S_OK)
                    {
                        // If dimensions match within a small threshold, skip the resize entirely
                        if (Math.Abs(origDesc.Width - sz.Width) <= 1 && Math.Abs(origDesc.Height - sz.Height) <= 1)
                        {
                            System.Diagnostics.Debug.WriteLine($"[DEBUG] Skipping resize - already at size {origDesc.Width}x{origDesc.Height}");
                            return HRESULT.S_OK;
                        }
                    }
                }
                catch (Exception ex)
                {
                    System.Diagnostics.Debug.WriteLine($"[DEBUG] Error checking swap chain dimensions: {ex.Message}");
                }
                
                // Start performance tracking for resize operation
                BeginTimeMeasure("Resize");
                
                try
                {
                    // Don't call BeginDraw/Clear/EndDraw - this reduces white flashes
                    
                    // First release the D2D target bitmap
                    if (m_pD2DDeviceContext != null && m_pD2DTargetBitmap != null)
                    {
                        m_pD2DDeviceContext.SetTarget(null);
                    }
                    
                    SafeRelease(ref m_pD2DTargetBitmap);
                    
                    // Don't present after clearing the target - this is a key difference 
                    // from the original Resize method to reduce flashing
                    
                    if (sz.Width > 0 && sz.Height > 0)
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

                            if ((uint)hr == 0x887A0001) // DXGI_ERROR_INVALID_CALL (buffer references exist)
                            {
                                // If buffer still has references, do a more aggressive cleanup
                                GC.Collect();
                                GC.WaitForPendingFinalizers();
                                System.Threading.Thread.Sleep(50);
                                
                                // Try resize again
                                hr = m_pDXGISwapChain1.ResizeBuffers(
                                    2,
                                    (uint)sz.Width,
                                    (uint)sz.Height,
                                    DXGI_FORMAT.DXGI_FORMAT_B8G8R8A8_UNORM,
                                    0
                                );
                                
                                if (hr != HRESULT.S_OK)
                                {
                                    // Last resort - recreate the swap chain
                                    SafeRelease(ref m_pDXGISwapChain1);
                                    hr = CreateSwapChain(IntPtr.Zero, (uint)sz.Width, (uint)sz.Height);
                                }
                            }

                            // Configure the swap chain - create the target bitmap
                            if (hr == HRESULT.S_OK)
                            {
                                hr = ConfigureSwapChain();
                            }
                            
                            // Notify the renderer about the new size
                            if (_isActive && _d2dRenderer != null && hr == HRESULT.S_OK)
                            {
                                _d2dRenderer.Resize((uint)sz.Width, (uint)sz.Height);
                                
                                // Mark that we need to re-render but don't do it immediately
                                // This allows the next CompositionTarget_Rendering to pick it up naturally
                                _rendered = false;
                            }
                        }
                        catch (Exception ex)
                        {
                            System.Diagnostics.Debug.WriteLine($"[DEBUG] Exception during resize: {ex.Message}");
                        }
                    }
                }
                finally
                {
                    // End performance tracking regardless of success or failure
                    EndTimeMeasure("Resize");
                }
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
                            // System.Threading.Thread.Sleep(50);
                            
                            // Start performance tracking for render operation
                            BeginTimeMeasure("Render");
                            
                            _d2dRenderer.Render(_markdown);
                            _rendered = true;
                            
                            // End performance tracking
                            EndTimeMeasure("Render");
                            
                            // After successful render, add another small delay before ticking 
                            // to ensure everything is properly set up
                            // System.Threading.Thread.Sleep(16);
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
                // Implement throttling to limit rendering to 60fps
                long currentTimestamp = GetHighPrecisionTimestamp();
                if (_lastFrameTimestamp > 0)
                {
                    float elapsedMs = ConvertToMilliseconds(_lastFrameTimestamp, currentTimestamp);
                    
                    // If not enough time has passed since last frame, skip this one
                    if (elapsedMs < _minimumFrameTimeMs)
                    {
                        // Skip rendering this frame to maintain ~60fps
                        System.Diagnostics.Debug.WriteLine($"[PERF] Throttling frame - only {elapsedMs:F2}ms elapsed (target: {_minimumFrameTimeMs:F2}ms)");
                        return;
                    }
                }
                
                // Update the timestamp for the next frame
                _lastFrameTimestamp = currentTimestamp;
                
                // Increment total frame counter for statistics
                _totalFrames++;
                
                // Check if we're already rendering a frame - implement frame dropping
                if (_isRenderingFrame)
                {
                    // Calculate how long the current frame has been rendering
                    long currentTime = GetHighPrecisionTimestamp();
                    float frameRenderTimeMs = ConvertToMilliseconds(_renderingStartTime, currentTime);
                    
                    // If we've been rendering for too long, drop this frame
                    if (frameRenderTimeMs > _targetFrameTimeMs)
                    {
                        _droppedFrameCount++;
                        _consecutiveDroppedFrames++;
                        
                        // Log frame dropping but don't spam the console
                        if (_consecutiveDroppedFrames == 1 || _consecutiveDroppedFrames % 10 == 0)
                        {
                            System.Diagnostics.Debug.WriteLine($"[PERF] Dropping frame #{_totalFrames} - previous frame still rendering for {frameRenderTimeMs:F2}ms. Total dropped: {_droppedFrameCount}");
                        }
                        
                        // If we've dropped too many consecutive frames, force through the next one to prevent complete stalling
                        if (_consecutiveDroppedFrames >= _maxConsecutiveDroppedFrames)
                        {
                            System.Diagnostics.Debug.WriteLine($"[PERF] Force rendering after {_consecutiveDroppedFrames} consecutive dropped frames");
                        }
                        else
                        {
                            // Skip this frame
                            return;
                        }
                    }
                }
                
                // Start tracking this new frame's rendering
                _renderingStartTime = GetHighPrecisionTimestamp();
                _isRenderingFrame = true;
                
                // Reset consecutive dropped frames counter since we're rendering this frame
                _consecutiveDroppedFrames = 0;
                
                // Start performance tracking for frame
                BeginTimeMeasure("Total");
                
                // First, ensure we have properly configured Direct2D resources and a valid target
                bool targetIsNull = false;
                
                // CRITICAL FIX: Check if the target is null with proper error handling
                try
                {
                    ID2D1Image currentTarget = null;
                    if (m_pD2DDeviceContext != null)
                    {
                        m_pD2DDeviceContext.GetTarget(out currentTarget);
                        targetIsNull = (currentTarget == null);
                        if (currentTarget != null)
                        {
                            SafeRelease(ref currentTarget);
                        }
                    }
                }
                catch (Exception ex)
                {
                    System.Diagnostics.Debug.WriteLine($"[DEBUG] Error checking target: {ex.Message}");
                    targetIsNull = true; // Assume target is null if we got an exception
                }
                
                bool targetBitmapIsNull = (m_pD2DTargetBitmap == null);
                bool targetNeedsReconfiguring = (targetIsNull || targetBitmapIsNull) && m_pDXGISwapChain1 != null;
                
                // CIRCUIT BREAKER: Prevent infinite reconfiguration loop
                TimeSpan timeSinceLastReconfiguration = DateTime.Now - _lastReconfigurationTime;
                bool canReconfigure = timeSinceLastReconfiguration > _reconfigurationCooldown;
                
                if (targetNeedsReconfiguring)
                {
                    // Check if we've hit the reconfiguration limit
                    if (_targetReconfigurationAttempts >= _maxReconfigurationAttempts && !canReconfigure)
                    {
                        System.Diagnostics.Debug.WriteLine($"[CRITICAL] Circuit breaker activated - too many reconfiguration attempts ({_targetReconfigurationAttempts}). Skipping rendering until cooldown period elapsed.");
                        return;
                    }
                    
                    // If we're past the cooldown period, reset the counter
                    if (canReconfigure)
                    {
                        _targetReconfigurationAttempts = 0;
                    }
                    
                    // Track reconfiguration attempts and time
                    _targetReconfigurationAttempts++;
                    _lastReconfigurationTime = DateTime.Now;
                    
                    // If target bitmap is missing but we have a swap chain, try to reconfigure
                    System.Diagnostics.Debug.WriteLine($"Attempting to reconfigure swap chain due to missing target. targetIsNull={targetIsNull}, targetBitmapIsNull={targetBitmapIsNull} (Attempt {_targetReconfigurationAttempts} of {_maxReconfigurationAttempts})");
                    
                    // CRITICAL FIX: Make sure the device context doesn't have a target set before reconfiguring
                    if (m_pD2DDeviceContext != null && !targetIsNull)
                    {
                        try
                        {
                            m_pD2DDeviceContext.SetTarget(null);
                        }
                        catch (Exception ex)
                        {
                            System.Diagnostics.Debug.WriteLine($"[DEBUG] Error clearing target before reconfigure: {ex.Message}");
                        }
                    }
                    
                    // Release the target bitmap if it exists
                    if (m_pD2DTargetBitmap != null)
                    {
                        SafeRelease(ref m_pD2DTargetBitmap);
                    }
                    
                    // Wait before reconfiguring
                    // System.Threading.Thread.Sleep(50);
                    
                    hr = ConfigureSwapChain();
                    if (hr != HRESULT.S_OK)
                    {
                        System.Diagnostics.Debug.WriteLine($"Failed to reconfigure swap chain: 0x{hr:X}");
                        // Don't attempt to render if we couldn't configure the swap chain
                        return;
                    }
                    else
                    {
                        // Recheck if the target is still null after reconfiguring
                        try
                        {
                            ID2D1Image recheckTarget = null;
                            if (m_pD2DDeviceContext != null)
                            {
                                m_pD2DDeviceContext.GetTarget(out recheckTarget);
                                targetIsNull = (recheckTarget == null);
                                if (recheckTarget != null)
                                {
                                    SafeRelease(ref recheckTarget);
                                }
                            }
                            
                            System.Diagnostics.Debug.WriteLine($"[DEBUG] After reconfigure: targetIsNull={targetIsNull}, m_pD2DTargetBitmap is {(m_pD2DTargetBitmap == null ? "null" : "valid")}");
                            
                            // If target is still null but we have a valid bitmap, try to set the target again
                            if (targetIsNull && m_pD2DTargetBitmap != null)
                            {
                                System.Diagnostics.Debug.WriteLine("[DEBUG] Setting target again after reconfigure");
                                m_pD2DDeviceContext.SetTarget(m_pD2DTargetBitmap);
                                
                                // Verify the target was set
                                ID2D1Image verifyTarget = null;
                                m_pD2DDeviceContext.GetTarget(out verifyTarget);
                                if (verifyTarget != null)
                                {
                                    System.Diagnostics.Debug.WriteLine("[DEBUG] Target set successfully after reconfigure");
                                    SafeRelease(ref verifyTarget);
                                    targetIsNull = false;
                                    
                                    // Reset the counter since we were successful
                                    _targetReconfigurationAttempts = 0;
                                }
                                else
                                {
                                    System.Diagnostics.Debug.WriteLine("[DEBUG] Target still null after attempt to set it!");
                                }
                            }
                        }
                        catch (Exception ex)
                        {
                            System.Diagnostics.Debug.WriteLine($"[DEBUG] Error checking/setting target after reconfigure: {ex.Message}");
                            targetIsNull = true;
                        }
                    }
                }
                
                // Extra validation to prevent NULL target rendering
                if (targetIsNull || m_pD2DTargetBitmap == null)
                {
                    System.Diagnostics.Debug.WriteLine($"Target bitmap is null or not set - cannot render. targetIsNull={targetIsNull}, m_pD2DTargetBitmap is {(m_pD2DTargetBitmap == null ? "null" : "valid")}");
                    return; 
                }
                
                // Now proceed with rendering since we have a valid target
                bool needsPresent = false;

                // For the first frame or if not yet rendered, make sure to show something
                if (!_rendered && m_pD2DDeviceContext != null)
                {
                    try 
                    {
                        // Verify target is set before trying to render initial frame
                        ID2D1Image checkTarget = null;
                        m_pD2DDeviceContext.GetTarget(out checkTarget);
                        
                        if (checkTarget != null)
                        {
                            SafeRelease(ref checkTarget);
                            
                            // No need to call BeginDraw/EndDraw here anymore - let d2drender.rs handle it
                            System.Diagnostics.Debug.WriteLine("[DEBUG] First frame, setting needs_render flag to true");
                            
                            // Set a flag to trigger Tick() rendering
                            needsPresent = true;
                        }
                        else
                        {
                            System.Diagnostics.Debug.WriteLine("[DEBUG] WARNING: First frame has NULL target");
                            // Skip rendering this frame
                            return;
                        }
                    }
                    catch (Exception ex)
                    {
                        System.Diagnostics.Debug.WriteLine($"Error checking target for initial frame: {ex.Message}");
                        return; // Skip the rest of rendering for this frame
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
                        BeginTimeMeasure("Tick");
                        _d2dRenderer.Tick();
                        EndTimeMeasure("Tick");
                        
                        needsPresent = true; // We should present after a successful Tick
                    }
                    catch (Exception ex)
                    {
                        System.Diagnostics.Debug.WriteLine($"Exception in Tick: {ex.Message}");
                        
                        // CRITICAL FIX: Handle NULL target error more aggressively
                        if (ex.Message.Contains("NULL target"))
                        {
                            System.Diagnostics.Debug.WriteLine("[DEBUG] NULL target error in Tick, attempting immediate recovery");
                            
                            if (m_pD2DDeviceContext != null && m_pD2DTargetBitmap != null)
                            {
                                try
                                {
                                    // Force target reset
                                    m_pD2DDeviceContext.SetTarget(null);
                                    // System.Threading.Thread.Sleep(50);
                                    m_pD2DDeviceContext.SetTarget(m_pD2DTargetBitmap);
                                    
                                    // Verify the target was set
                                    ID2D1Image verifyTarget = null;
                                    m_pD2DDeviceContext.GetTarget(out verifyTarget);
                                    if (verifyTarget != null)
                                    {
                                        System.Diagnostics.Debug.WriteLine("[DEBUG] Target successfully reset after NULL target error");
                                        SafeRelease(ref verifyTarget);
                                        needsPresent = true;
                                        System.Diagnostics.Debug.WriteLine("[DEBUG] Drew white background after recovery");
                                    }
                                    else
                                    {
                                        System.Diagnostics.Debug.WriteLine("[DEBUG] Failed to reset target after NULL target error");
                                    }
                                }
                                catch (Exception recovery_ex)
                                {
                                    System.Diagnostics.Debug.WriteLine($"[DEBUG] Error during NULL target recovery: {recovery_ex.Message}");
                                }
                            }
                            else
                            {
                                System.Diagnostics.Debug.WriteLine("[DEBUG] Cannot recover - device context or target bitmap is null");
                                // Force complete reconfiguration on next frame
                                SafeRelease(ref m_pD2DTargetBitmap);
                            }
                        }
                    }
                }
                else
                {
                    System.Diagnostics.Debug.WriteLine("[DEBUG] Skipping Tick due to NULL target");
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
                            
                            BeginTimeMeasure("Present");
                            hr = m_pDXGISwapChain1.Present(1, 0); // Use vsync (1) for smoother rendering
                            EndTimeMeasure("Present");
                            
                            if (hr == HRESULT.S_OK)
                            {
                                _rendered = true;
                            }
                            else if ((uint)hr == D2DTools.D2DERR_RECREATE_TARGET)
                            {
                                System.Diagnostics.Debug.WriteLine("[DEBUG] Need to recreate rendering target");
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
                            System.Diagnostics.Debug.WriteLine("[DEBUG] Skipping Present due to NULL target");
                        }
                    }
                    catch (Exception ex)
                    {
                        System.Diagnostics.Debug.WriteLine($"Error during final target check or present: {ex.Message}");
                    }
                }
                
                // End performance tracking for this frame
                EndTimeMeasure("Total");
                
                // Reset the rendering flag since we're done with this frame
                _isRenderingFrame = false;
                
                // Update performance statistics and metrics
                UpdatePerformanceStatistics();
            }
            catch (Exception ex)
            {
                System.Diagnostics.Debug.WriteLine($"Error in CompositionTarget_Rendering: {ex.Message} (0x{ex.HResult:X8})");
                
                // CRITICAL FIX: Enhanced recovery for rendering errors
                try
                {
                    System.Diagnostics.Debug.WriteLine("[DEBUG] Attempting to recover from rendering error");
                    
                    // Reset all Direct2D resources
                    if (m_pD2DDeviceContext != null)
                    {
                        try 
                        { 
                            m_pD2DDeviceContext.SetTarget(null); 
                        }
                        catch { }
                    }
                    
                    SafeRelease(ref m_pD2DTargetBitmap);
                    
                    // Wait a bit before recreating resources
                    // System.Threading.Thread.Sleep(100);
                    
                    // Force GC
                    GC.Collect();
                    GC.WaitForPendingFinalizers();
                    
                    // CIRCUIT BREAKER: Only attempt recovery if we haven't exceeded our limit
                    if (_targetReconfigurationAttempts < _maxReconfigurationAttempts || 
                        (DateTime.Now - _lastReconfigurationTime) > _reconfigurationCooldown)
                    {
                        if (m_pDXGISwapChain1 != null)
                        {
                            _targetReconfigurationAttempts++;
                            _lastReconfigurationTime = DateTime.Now;
                            
                            hr = ConfigureSwapChain();
                            if (hr == HRESULT.S_OK)
                            {
                                System.Diagnostics.Debug.WriteLine("[DEBUG] Resource recovery successful");
                                _rendered = false; // Force a redraw
                            }
                        }
                    }
                    else
                    {
                        System.Diagnostics.Debug.WriteLine("[CRITICAL] Circuit breaker preventing recovery attempt - too many recent attempts");
                    }
                }
                catch (Exception recovery_ex)
                {
                    System.Diagnostics.Debug.WriteLine($"Recovery failed: {recovery_ex.Message}");
                    _isActive = false;
                }
            }
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
}
