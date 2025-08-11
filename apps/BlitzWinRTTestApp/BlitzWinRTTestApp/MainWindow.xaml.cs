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
            var width = (uint)Math.Max(1, BlitzPanel.ActualWidth * scale);
            var height = (uint)Math.Max(1, BlitzPanel.ActualHeight * scale);
            
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
            var width = (uint)Math.Max(1, e.NewSize.Width * scale);
            var height = (uint)Math.Max(1, e.NewSize.Height * scale);
            
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
    }
}
