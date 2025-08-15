using BlitzWinRTTestApp.ViewModel;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.UI.Xaml.Controls;

namespace BlitzWinRTTestApp.View;

public sealed partial class SinglePage : Page
{
    public SettingsViewModel Settings { get; set; }

    public string HTML = EmbeddedContent.DemoHtml;

    public SinglePage()
    {
        Settings = ((App)App.Current).Services.GetService<SettingsViewModel>();
        InitializeComponent();
    }
}
