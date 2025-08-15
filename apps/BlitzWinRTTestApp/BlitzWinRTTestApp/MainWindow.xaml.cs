using BlitzWinRTTestApp.View;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

namespace BlitzWinRTTestApp;

/// <summary>
/// An empty window that can be used on its own or navigated to within a Frame.
/// </summary>
public sealed partial class MainWindow : Window
{
    public MainWindow()
    {
        InitializeComponent();
    }

    private void nvSample_SelectionChanged(NavigationView sender, NavigationViewSelectionChangedEventArgs args)
    {
        if (args.SelectedItemContainer is NavigationViewItem selectedItem)
        {
            // Navigate to the page associated with the selected item
            switch (selectedItem.Tag)
            {
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
