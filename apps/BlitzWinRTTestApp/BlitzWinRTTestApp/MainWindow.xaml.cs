using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Runtime.InteropServices.WindowsRuntime;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Data;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Navigation;
using Windows.Foundation;
using Windows.Foundation.Collections;
using BlitzWinUI;
using BlitzWinRTTestApp.Interop;
using Microsoft.UI.Dispatching;
using System.Diagnostics;
using System.Threading.Tasks;

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

                // Load some HTML with vibrant colors to easily see when rendering works
                string htmlContent = @"
                <html>
                <head>
                    <style>
                        body {
                            background: linear-gradient(135deg, #ff5f6d, #ffc371);
                            color: white;
                            font-family: 'Segoe UI', sans-serif;
                            margin: 0;
                            padding: 20px;
                            height: 100vh;
                            display: flex;
                            flex-direction: column;
                            justify-content: center;
                            align-items: center;
                            text-align: center;
                        }
                        h1 {
                            font-size: 48px;
                            margin-bottom: 10px;
                            text-shadow: 2px 2px 4px rgba(0,0,0,0.3);
                        }
                        p {
                            font-size: 24px;
                            max-width: 600px;
                        }
                    </style>
                </head>
                <body>
                    <h1>Blitz WinUI Integration</h1>
                    <p>SwapChainPanel is now working correctly!</p>
                </body>
                </html>";
                
                _host.LoadHtml(htmlContent);
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
    }
}
