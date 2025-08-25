using BlitzWinRTTestApp.ViewModel;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.UI.Xaml.Controls;
using Microsoft.Web.WebView2.Core;
using System;
using System.Threading.Tasks;

namespace BlitzWinRTTestApp.View;

public sealed partial class GalleryComparePage : Page
{
    public SettingsViewModel Settings { get; set; }
    public string Html { get; set; } = EmbeddedContent.HtmlGallery;

    public GalleryComparePage()
    {
        Settings = ((App)App.Current).Services.GetService<SettingsViewModel>();
        InitializeComponent();
    }

    private async Task InitializeWebViewAsync()
    {
        try
        {
            // Ensure CoreWebView2 created
            if (WebV.CoreWebView2 == null)
            {
                await WebV.EnsureCoreWebView2Async();
            }
            WebV.CoreWebView2.NavigateToString(Html);
        }
        catch (Exception ex)
        {
            // Minimal inline error surface; in a larger app we'd route this through logging infra.
            // For quick diagnostics show a simple HTML error page.
            if (WebV.CoreWebView2 == null)
            {
                try { await WebV.EnsureCoreWebView2Async(); } catch { /* swallow secondary failure */ }
            }
            try
            {
                WebV.NavigateToString($"<html><body><pre style='font-family:consolas'>WebView init failed:\n{System.Net.WebUtility.HtmlEncode(ex.ToString())}</pre></body></html>");
            }
            catch { /* ignore */ }
        }
    }

    private void WebV_Loaded(object sender, Microsoft.UI.Xaml.RoutedEventArgs e)
    {
        _ = InitializeWebViewAsync();
    }
}
