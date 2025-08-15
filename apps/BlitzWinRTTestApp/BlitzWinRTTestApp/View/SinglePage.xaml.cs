using Microsoft.UI.Xaml.Controls;

namespace BlitzWinRTTestApp.View;

public sealed partial class SinglePage : Page
{
    public string HTML = EmbeddedContent.DemoHtml;
    public SinglePage()
    {
        InitializeComponent();
    }
}
