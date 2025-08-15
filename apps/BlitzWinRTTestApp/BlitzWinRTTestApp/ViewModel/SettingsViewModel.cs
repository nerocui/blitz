using CommunityToolkit.Mvvm.ComponentModel;
namespace BlitzWinRTTestApp.ViewModel;

public class SettingsViewModel : ObservableObject
{
    private bool _debugOverlayEnabled;
    public bool DebugOverlayEnabled
    {
        get => _debugOverlayEnabled;
        set => SetProperty(ref _debugOverlayEnabled, value);
    }
}
