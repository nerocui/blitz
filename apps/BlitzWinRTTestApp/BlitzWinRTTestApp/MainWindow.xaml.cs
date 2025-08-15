using Microsoft.UI.Xaml;

namespace BlitzWinRTTestApp;

/// <summary>
/// An empty window that can be used on its own or navigated to within a Frame.
/// </summary>
public sealed partial class MainWindow : Window
{
    public string HTML = EmbeddedContent.DemoHtml;
    public string Card1HTML { get; } = "<html><body style='margin:0;font-family:Segoe UI,system-ui;background:linear-gradient(135deg,#2d2f33,#1f2225);color:#fff;display:flex;flex-direction:column;justify-content:center;align-items:center;height:100%;'><h2 style='margin:4px 0;font-size:20px;'>System Status</h2><div style='font-size:12px;opacity:.75'>All services nominal</div></body></html>";
    public string Card2HTML { get; } = "<html><body style='margin:0;font-family:Segoe UI;background:#1E1B2B;color:#E0DFF7;display:flex;flex-direction:column;padding:12px;'><div style='font-size:12px;letter-spacing:.08em;text-transform:uppercase;opacity:.6;'>Build</div><div style='font-size:42px;font-weight:600;line-height:1;'>✅</div><div style='margin-top:auto;font-size:12px;opacity:.7;'>Last: &lt;2 min ago</div></body></html>";
    public string Card3HTML { get; } = "<html><body style='margin:0;font-family:Segoe UI;background:#14242E;color:#D8F3FF;display:flex;flex-direction:column;padding:14px;'><h3 style='margin:0 0 6px;font:600 16px Segoe UI'>Throughput</h3><div style='font:500 36px Segoe UI'>842<span style='font-size:14px;margin-left:4px;opacity:.6'>r/s</span></div><div style='margin-top:auto;font-size:11px;opacity:.65'>Last 60s window</div></body></html>";
    public string Card4HTML { get; } = "<html><body style='margin:0;font-family:Segoe UI;background:radial-gradient(circle at 30% 30%,#30364A,#191C25);color:#fff;display:flex;flex-direction:column;padding:16px;'><div style='font-size:12px;opacity:.6;'>Memory</div><div style='margin-top:4px;font-size:32px;font-weight:600;'>3.2<span style='font-size:14px;opacity:.7;margin-left:4px;'>GB</span></div><div style='margin-top:auto;width:100%;height:8px;background:#444;border-radius:4px;overflow:hidden;'><div style='height:100%;width:48%;background:linear-gradient(90deg,#56CCF2,#2F80ED);'></div></div></body></html>";
    public string Card5HTML { get; } = "<html><body style='margin:0;font-family:Segoe UI;background:linear-gradient(160deg,#113B2C,#0C221B);color:#DEFBE6;display:flex;flex-direction:column;padding:16px;'><div style='font-size:12px;opacity:.65;'>Latency (p95)</div><div style='margin-top:6px;font-size:34px;font-weight:600;'>18<span style='font-size:14px;margin-left:4px;opacity:.7'>ms</span></div><div style='margin-top:auto;font-size:11px;opacity:.55;'>Stable range</div></body></html>";
    public string Card6HTML { get; } = "<html><body style='margin:0;font-family:Segoe UI;background:linear-gradient(145deg,#3B1212,#1E0A0A);color:#FFD7D7;display:flex;flex-direction:column;padding:16px;'><div style='font-size:12px;opacity:.65;'>Errors (1h)</div><div style='margin-top:6px;font-size:34px;font-weight:600;'>0<span style='font-size:14px;margin-left:4px;opacity:.6'>/min</span></div><div style='margin-top:auto;font-size:11px;opacity:.5;'>Excellent</div></body></html>";
    public string FeatureCardHTML { get; } = "<html><body style='margin:0;font-family:Segoe UI,system-ui;background:#121212;color:#F5F5F5;display:flex;flex-direction:row;height:100%;'>"+
        "<div style='flex:1;padding:28px;display:flex;flex-direction:column;'><h1 style='margin:0 0 12px;font:600 28px Segoe UI;background:linear-gradient(90deg,#56CCF2,#2F80ED);-webkit-background-clip:text;color:transparent;'>Blitz</h1>"+
        "<p style='margin:0 0 16px;line-height:1.4;font-size:14px;opacity:.85'>High‑performance HTML/CSS layout &amp; rendering embedded directly in your WinUI app.</p>"+
        "<ul style='margin:0;padding-left:18px;font-size:12px;opacity:.75;line-height:1.5'><li>GPU accelerated</li><li>Deterministic layout</li><li>Composable rendering</li></ul>"+
        "<div style='margin-top:auto;font-size:11px;opacity:.55'>Prototype build</div></div>"+
        "<div style='width:340px;display:flex;align-items:center;justify-content:center;background:radial-gradient(circle at 40% 40%,#203040,#0d141a);'><div style='font-size:72px;filter:drop-shadow(0 4px 8px rgba(0,0,0,.6));'>⚡</div></div>"+
        "</body></html>";

    public MainWindow()
    {
        InitializeComponent();
    }
}
