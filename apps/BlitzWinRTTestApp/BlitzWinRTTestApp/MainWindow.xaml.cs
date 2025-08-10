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

                // Load some HTML with vibrant colors to easily see when rendering works
                string htmlContent = @"
                <html>
                <head>
                    <style>
                        :root { --bg1:#ff5f6d; --bg2:#ffc371; }
                        body {
                            background: linear-gradient(135deg, var(--bg1), var(--bg2));
                            color: #fff;
                            font-family: 'Segoe UI', sans-serif;
                            margin: 0;
                            padding: 32px 32px 120px 32px;
                            min-height: 100vh;
                            box-sizing: border-box;
                        }
                        h1 { font-size: 48px; margin: 0 0 4px; text-shadow: 2px 2px 4px rgba(0,0,0,.3); }
                        p.lead { font-size: 20px; margin: 0 0 32px; max-width: 760px; }
                        .shadow-grid {
                            display: flex;
                            flex-wrap: wrap;
                            gap: 28px;
                            max-width: 1200px;
                        }
                        .shadow {
                            width: 200px;
                            height: 120px;
                            padding: 12px 14px;
                            border-radius: 12px;
                            background: #fff;
                            color: #222;
                            display: flex;
                            flex-direction: column;
                            justify-content: flex-end;
                            font-size: 14px;
                            position: relative;
                            box-sizing: border-box;
                        }
                        .shadow span { font-size: 11px; opacity: .65; line-height: 1.2; }
                        /* Various box-shadow combinations to exercise radius & std-dev pathways */
                        .s1 { box-shadow: 0 2px 4px rgba(0,0,0,.25); }
                        .s2 { box-shadow: 0 4px 12px rgba(0,0,0,.30); }
                        .s3 { box-shadow: 0 8px 24px rgba(0,0,0,.30); }
                        .s4 { box-shadow: 0 12px 32px rgba(0,0,0,.35), 0 2px 4px rgba(0,0,0,.2); }
                        .s5 { box-shadow: 0 0 0 1px rgba(0,0,0,.05), 0 16px 48px -8px rgba(0,0,0,.45); }
                        .inset { box-shadow: inset 0 0 8px rgba(0,0,0,.35); background: #fafafa; }
                        /* Colored shadows */
                        .c1 { box-shadow: 0 10px 30px -5px rgba(255,95,109,0.55); }
                        .c2 { box-shadow: 0 10px 40px -4px rgba(60,140,255,0.55); }
                        footer { position: fixed; bottom: 12px; left: 0; right:0; text-align:center; font-size:12px; opacity:.7; }
                        code { background: rgba(255,255,255,.2); padding:2px 4px; border-radius:4px; }
                    </style>
                </head>
                <body>
                    <h1>Blitz WinUI Integration</h1>
                    <p class='lead'>SwapChainPanel + Direct2D backend. Below are live <code>box-shadow</code> examples to verify Gaussian blur shadow rendering.</p>
                    <div class='shadow-grid'>
                        <div class='shadow s1'><strong>shadow 1</strong><span>0 2px 4px</span></div>
                        <div class='shadow s2'><strong>shadow 2</strong><span>0 4px 12px</span></div>
                        <div class='shadow s3'><strong>shadow 3</strong><span>0 8px 24px</span></div>
                        <div class='shadow s4'><strong>layered</strong><span>0 12px 32px, 0 2px 4px</span></div>
                        <div class='shadow s5'><strong>elevated</strong><span>ring + deep</span></div>
                        <div class='shadow inset'><strong>inset</strong><span>inset 0 0 8px</span></div>
                        <div class='shadow c1'><strong>warm</strong><span>colored glow</span></div>
                        <div class='shadow c2'><strong>cool</strong><span>colored glow</span></div>
                    </div>
                    <footer>Box shadow demo &middot; Adjust values in MainWindow.xaml.cs to test radius/std-dev mapping.</footer>
                </body>
                </html>";
                
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
