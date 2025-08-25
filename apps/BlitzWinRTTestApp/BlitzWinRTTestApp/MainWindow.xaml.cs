using BlitzWinRTTestApp.View;
using BlitzWinRTTestApp.ViewModel;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

namespace BlitzWinRTTestApp;

/// <summary>
/// An empty window that can be used on its own or navigated to within a Frame.
/// </summary>
public sealed partial class MainWindow : Window
{
    public SettingsViewModel Settings { get; set; }
    public MainWindow()
    {
        Settings = ((App)App.Current).Services.GetService<SettingsViewModel>();
        InitializeComponent();
    }

    private void nvSample_SelectionChanged(NavigationView sender, NavigationViewSelectionChangedEventArgs args)
    {
        if (args.SelectedItemContainer is NavigationViewItem selectedItem)
        {
            // Navigate to the page associated with the selected item
            switch (selectedItem.Tag)
            {
                case "Gallery":
                    contentFrame.Navigate(typeof(GalleryComparePage));
                    break;
                case "SinglePage":
                    contentFrame.Navigate(typeof(SinglePage));
                    break;
                case "CardsPage":
                    contentFrame.Navigate(typeof(CardsPage));
                    break;
                default:
                    break;
            }
        }
    }
}
