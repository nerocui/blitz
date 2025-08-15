using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.Extensions.DependencyInjection;
using System;
using BlitzWinRTTestApp.ViewModel;

namespace BlitzWinRTTestApp;

public partial class App : Application
{
    private Window? _window;

    public App()
    {
        Services = ConfigureServices();
        InitializeComponent();
    }

    public IServiceProvider Services { get; }

    private static IServiceProvider ConfigureServices()
    {
        var services = new ServiceCollection();

        services.AddSingleton<SettingsViewModel>();

        return services.BuildServiceProvider();
    }

    protected override void OnLaunched(LaunchActivatedEventArgs args)
    {
        _window = new MainWindow();
        _window.ExtendsContentIntoTitleBar = true;
        _window.AppWindow.TitleBar.PreferredHeightOption = TitleBarHeightOption.Tall;
        _window.Activate();
    }
}
