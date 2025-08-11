using BlitzWinRTTestApp.Interop;
using BlitzWinUI;
using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Data;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Navigation;
// using Microsoft.UI.Input; // Not needed after simplifying modifier detection
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Runtime.InteropServices.WindowsRuntime;
using System.Threading.Tasks;
using Windows.ApplicationModel;
using Windows.Foundation;
using Windows.Foundation.Collections;
using Windows.Storage;

// To learn more about WinUI, the WinUI project structure,
// and more about our project templates, see: http://aka.ms/winui-project-info.

namespace BlitzWinRTTestApp
{
    /// <summary>
    /// An empty window that can be used on its own or navigated to within a Frame.
    /// </summary>
    public sealed partial class MainWindow : Window
    {
        private Host? _host;
        private SwapChainAttacher? _attacher;
        private DispatcherQueue _dispatcherQueue;
    private bool _pendingVerboseToggle;

        public MainWindow()
        {
            InitializeComponent();
            _dispatcherQueue = DispatcherQueue.GetForCurrentThread();
            
            // Set up a logging textbox to see what's happening
            LogMessages("Application started");

            // Hook pointer events early (panel may not be loaded yet; events added again after load if needed)
            BlitzPanel.PointerMoved += BlitzPanel_PointerMoved;
            BlitzPanel.PointerPressed += BlitzPanel_PointerPressed;
            BlitzPanel.PointerReleased += BlitzPanel_PointerReleased;
            BlitzPanel.PointerWheelChanged += BlitzPanel_PointerWheelChanged;
        }

        private void LogMessages(string message)
        {
            Debug.WriteLine(message);
            
            // Also append to LogTextBox if it exists
            _dispatcherQueue.TryEnqueue(() => 
            {
                if (LogTextBox != null)
                {
                    LogTextBox.Text += $"{DateTime.Now:HH:mm:ss.fff}: {message}\n";
                    LogScrollViewer.ChangeView(null, double.MaxValue, null);
                }
            });
        }

        private async void BlitzPanel_Loaded(object sender, RoutedEventArgs e)
        {
            LogMessages("BlitzPanel_Loaded: Starting panel initialization");
            
            var scale = (float)BlitzPanel.XamlRoot.RasterizationScale;
            var width = (uint)Math.Max(1, BlitzPanel.ActualWidth);
            var height = (uint)Math.Max(1, BlitzPanel.ActualHeight);
            
            LogMessages($"BlitzPanel_Loaded: Panel size: {width}x{height}, scale: {scale}");

            try
            {
                _attacher = new SwapChainAttacher(BlitzPanel);
                LogMessages($"BlitzPanel_Loaded: Created attacher: {_attacher}");
                
                // Wait briefly to ensure the panel is fully initialized
                await Task.Delay(100);
                
                // First explicitly test the attacher works correctly
                LogMessages("BlitzPanel_Loaded: Testing TestAttacherConnection method on attacher directly");
                var directTest = _attacher.TestAttacherConnection();
                LogMessages($"BlitzPanel_Loaded: Direct test result: {directTest}");
                
                LogMessages("BlitzPanel_Loaded: Creating Host with attacher");
                _host = new Host(_attacher, width, height, scale);
                LogMessages($"BlitzPanel_Loaded: Host created successfully: {_host}");

                // If user toggled verbose before host creation, apply now
                if (_pendingVerboseToggle)
                {
                    try { _host.SetVerboseLogging(true); LogMessages("Verbose logging enabled (deferred)"); }
                    catch (Exception vex) { LogMessages("Failed to enable verbose logging: " + vex.Message); }
                }

                // Test the connection through the host
                try
                {
                    LogMessages("BlitzPanel_Loaded: Testing attacher connection through Host");
                    var connectionResult = _host.TestAttacherConnection();
                    LogMessages($"BlitzPanel_Loaded: TestAttacherConnection result: {connectionResult}");
                }
                catch (Exception testEx)
                {
                    LogMessages($"BlitzPanel_Loaded: Connection test exception: {testEx.GetType().Name}: {testEx.Message}");
                }

                // Load HTML from packaged Assets (ms-appx URI). Works for packaged WinUI 3 apps.
                string htmlContent;
                try
                {
                    StorageFolder installedLocation = Package.Current.InstalledLocation;
                    StorageFolder assetsFolder = await installedLocation.GetFolderAsync("Assets");
                    StorageFile file = await assetsFolder.GetFileAsync("demo.html");

                    htmlContent = await FileIO.ReadTextAsync(file);
                    LogMessages($"Loaded HTML from Assets/demo.html ({htmlContent.Length} chars)");
                }
                catch (Exception loadEx)
                {
                    htmlContent = $"<html><body><h1>Asset Load Error</h1><p>{System.Net.WebUtility.HtmlEncode(loadEx.Message)}</p><p>Ensure Assets/demo.html is marked as Content.</p></body></html>";
                    LogMessages("Failed to load Assets/demo.html: " + loadEx.Message);
                }

                _host.LoadHtml(htmlContent);
                // _host.SetVerboseLogging(true); API to turn on verbose logging on the rust side
                LogMessages("BlitzPanel_Loaded: HTML loaded");

                // Render on XAML composition ticks
                CompositionTarget.Rendering += CompositionTarget_Rendering;
                LogMessages("BlitzPanel_Loaded: Render timer started");

                // Ensure event handlers attached (in case constructor ran before XAML name hookup)
                BlitzPanel.PointerMoved -= BlitzPanel_PointerMoved; BlitzPanel.PointerMoved += BlitzPanel_PointerMoved;
                BlitzPanel.PointerPressed -= BlitzPanel_PointerPressed; BlitzPanel.PointerPressed += BlitzPanel_PointerPressed;
                BlitzPanel.PointerReleased -= BlitzPanel_PointerReleased; BlitzPanel.PointerReleased += BlitzPanel_PointerReleased;
                BlitzPanel.PointerWheelChanged -= BlitzPanel_PointerWheelChanged; BlitzPanel.PointerWheelChanged += BlitzPanel_PointerWheelChanged;
            }
            catch (Exception ex)
            {
                LogMessages($"BlitzPanel_Loaded: Exception: {ex.GetType().Name}: {ex.Message}");
                LogMessages($"BlitzPanel_Loaded: Stack trace: {ex.StackTrace}");
            }
        }

        private void BlitzPanel_SizeChanged(object sender, SizeChangedEventArgs e)
        {
            if (_host is null) return;
            
            var scale = (float)BlitzPanel.XamlRoot.RasterizationScale;
            var width = (uint)Math.Max(1, e.NewSize.Width);
            var height = (uint)Math.Max(1, e.NewSize.Height);
            
            LogMessages($"BlitzPanel_SizeChanged: Resizing to {width}x{height}, scale: {scale}");
            _host.Resize(width, height, scale);
        }

        private void CompositionTarget_Rendering(object? sender, object e)
        {
            try
            {
                _host?.RenderOnce();
            }
            catch (Exception ex)
            {
                // Only log first rendering exception to avoid spamming
                CompositionTarget.Rendering -= CompositionTarget_Rendering;
                LogMessages($"Rendering error: {ex.Message}");
            }
        }

        private void VerboseToggle_Toggled(object sender, RoutedEventArgs e)
        {
            // Access ToggleSwitch via sender or name
            if (sender is ToggleSwitch ts)
            {
                var isOn = ts.IsOn;
                if (_host == null)
                {
                    _pendingVerboseToggle = isOn; // store for later
                    LogMessages("VerboseToggle: host not yet created; deferring apply (" + isOn + ")");
                    return;
                }
                try
                {
                    _host.SetVerboseLogging(isOn);
                    LogMessages("Verbose logging " + (isOn ? "enabled" : "disabled"));
                }
                catch (Exception ex2)
                {
                    LogMessages("VerboseToggle error: " + ex2.Message);
                }
            }
        }

        private void ClearLog_Click(object sender, RoutedEventArgs e)
        {
            LogTextBox.Text = string.Empty;
            LogMessages("Log cleared");
        }

        // --- Pointer / Wheel forwarding ---
        private void BlitzPanel_PointerMoved(object sender, PointerRoutedEventArgs e)
        {
            if (_host == null) return;
            var pt = e.GetCurrentPoint(BlitzPanel);
            uint modifiers = (uint)e.KeyModifiers; // Shift=1, Control=2, Alt=4, Windows=8 matches expected bit layout
            // Buttons bitmask: align with rust side (MouseEventButtons bits). Use left=1, right=2, middle=4, X1=8, X2=16
            uint buttons = 0;
            if (pt.Properties.IsLeftButtonPressed) buttons |= 1;
            if (pt.Properties.IsRightButtonPressed) buttons |= 2;
            if (pt.Properties.IsMiddleButtonPressed) buttons |= 4;
            if (pt.Properties.IsXButton1Pressed) buttons |= 8;
            if (pt.Properties.IsXButton2Pressed) buttons |= 16;
            _host.PointerMove((float)pt.Position.X, (float)pt.Position.Y, buttons, modifiers);
        }

        private void BlitzPanel_PointerPressed(object sender, PointerRoutedEventArgs e)
        {
            if (_host == null) return;
            var pt = e.GetCurrentPoint(BlitzPanel);
            byte button = 0; // map primary left=0, right=2, middle=1 maybe; keep 0 for left
            if (pt.Properties.IsRightButtonPressed) button = 2;
            else if (pt.Properties.IsMiddleButtonPressed) button = 1;
            uint buttons = 0;
            if (pt.Properties.IsLeftButtonPressed) buttons |= 1;
            if (pt.Properties.IsRightButtonPressed) buttons |= 2;
            if (pt.Properties.IsMiddleButtonPressed) buttons |= 4;
            if (pt.Properties.IsXButton1Pressed) buttons |= 8;
            if (pt.Properties.IsXButton2Pressed) buttons |= 16;
            uint modifiers = (uint)e.KeyModifiers;
            _host.PointerDown((float)pt.Position.X, (float)pt.Position.Y, button, buttons, modifiers);
        }

        private void BlitzPanel_PointerReleased(object sender, PointerRoutedEventArgs e)
        {
            if (_host == null) return;
            var pt = e.GetCurrentPoint(BlitzPanel);
            byte button = 0;
            // Determine released button heuristic (prefer left/right/middle sequence)
            if (!pt.Properties.IsLeftButtonPressed) button = 0;
            uint modifiers = (uint)e.KeyModifiers;
            _host.PointerUp((float)pt.Position.X, (float)pt.Position.Y, button, 0, modifiers);
        }

        private void BlitzPanel_PointerWheelChanged(object sender, PointerRoutedEventArgs e)
        {
            if (_host == null) return;
            var pt = e.GetCurrentPoint(BlitzPanel);
            // MouseWheelDelta units: multiples of 120 (Win32). We'll map one notch (±120) to 48 CSS px vertical.
            // Direction: In typical UX, positive delta means wheel up (scroll content up -> negative y offset). Adjust sign accordingly.
            int raw = pt.Properties.MouseWheelDelta; // +120 (away from user / wheel up)
            double linesPerNotch = 1.0; // could read system setting; keep simple
            double pixelsPerLine = 48.0; // tune later
            double dy = raw / 120.0 * linesPerNotch * pixelsPerLine; // Don't invert, both wheel and trackpad already behave correctly
            double dx = 0.0;
            bool shift = (e.KeyModifiers & Windows.System.VirtualKeyModifiers.Shift) != 0;
            if (shift) { dx = dy; dy = 0; }
            _host.WheelScroll(dx, dy);
            e.Handled = true;
        }
        // Removed InputKeyboardSource-based modifier helpers (not needed; using e.KeyModifiers which is reliable in WinUI3 desktop)
    }
}
