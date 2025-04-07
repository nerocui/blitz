using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Navigation;
using Microsoft.UI.Xaml.Input;
using Windows.System;
using Microsoft.UI.Xaml; // Add UIElement reference
using Microsoft.UI.Input; // For InputKeyboardSource
using Windows.UI.Core; // For CoreVirtualKeyStates enum

// To learn more about WinUI, the WinUI project structure,
// and more about our project templates, see: http://aka.ms/winui-project-info.

namespace MarkdownTest;

/// <summary>
/// An empty page that can be used on its own or navigated to within a Frame.
/// </summary>
public sealed partial class MarkdownPage : Page
{
    public MarkdownPage()
    {
        this.InitializeComponent();
        this.Loaded += MarkdownPage_Loaded;
    }

    private void MarkdownPage_Loaded(object sender, Microsoft.UI.Xaml.RoutedEventArgs e)
    {
        // Focus the SwapChainPanel to receive keyboard input
        scpD2D.Focus(Microsoft.UI.Xaml.FocusState.Programmatic);
        scpD2D.KeyDown += ScpD2D_KeyDown;
        scpD2D.KeyUp += ScpD2D_KeyUp;
        scpD2D.CharacterReceived += ScpD2D_CharacterReceived;
        scpD2D.PointerPressed += ScpD2D_PointerPressed;
        scpD2D.PointerReleased += ScpD2D_PointerReleased;
        scpD2D.PointerMoved += ScpD2D_PointerMoved;
        scpD2D.PointerWheelChanged += ScpD2D_PointerWheelChanged;
        scpD2D.LostFocus += ScpD2D_LostFocus;
        scpD2D.GotFocus += ScpD2D_GotFocus;
    }

    private void ScpD2D_GotFocus(object sender, Microsoft.UI.Xaml.RoutedEventArgs e)
    {
        D2DContext.OnFocus();
    }

    private void ScpD2D_LostFocus(object sender, Microsoft.UI.Xaml.RoutedEventArgs e)
    {
        D2DContext.OnBlur();
    }

    private void ScpD2D_PointerWheelChanged(object sender, PointerRoutedEventArgs e)
    {
        var point = e.GetCurrentPoint(scpD2D);
        D2DContext.OnMouseWheel((float)point.Properties.MouseWheelDelta, 0);
    }

    private void ScpD2D_PointerMoved(object sender, PointerRoutedEventArgs e)
    {
        var point = e.GetCurrentPoint(scpD2D);
        D2DContext.OnPointerMoved((float)point.Position.X, (float)point.Position.Y);
    }

    private void ScpD2D_PointerReleased(object sender, PointerRoutedEventArgs e)
    {
        var point = e.GetCurrentPoint(scpD2D);
        uint button = 0; // Left button by default
        if (point.Properties.IsRightButtonPressed)
            button = 2; // Right button
        else if (point.Properties.IsMiddleButtonPressed)
            button = 1; // Middle button
        D2DContext.OnPointerReleased((float)point.Position.X, (float)point.Position.Y, button);
    }

    private void ScpD2D_PointerPressed(object sender, PointerRoutedEventArgs e)
    {
        var point = e.GetCurrentPoint(scpD2D);
        uint button = 0; // Left button by default
        if (point.Properties.IsRightButtonPressed)
            button = 2; // Right button
        else if (point.Properties.IsMiddleButtonPressed)
            button = 1; // Middle button
        D2DContext.OnPointerPressed((float)point.Position.X, (float)point.Position.Y, button);
        
        // Ensure we keep focus for keyboard input
        scpD2D.Focus(Microsoft.UI.Xaml.FocusState.Pointer);
    }

    private void ScpD2D_CharacterReceived(object sender, Microsoft.UI.Xaml.Input.CharacterReceivedRoutedEventArgs e)
    {
        // Forward text input to the renderer
        D2DContext.OnTextInput(e.Character.ToString());
        
        // Prevent default handling to avoid double input
        e.Handled = true;
    }

    private void ScpD2D_KeyUp(object sender, KeyRoutedEventArgs e)
    {
        D2DContext.OnKeyUp((uint)e.Key);
        e.Handled = true;
    }

    private void ScpD2D_KeyDown(object sender, KeyRoutedEventArgs e)
    {
        bool ctrl = IsKeyPressed(VirtualKey.Control);
        bool shift = IsKeyPressed(VirtualKey.Shift); 
        bool alt = IsKeyPressed(VirtualKey.Menu);
        
        D2DContext.OnKeyDown((uint)e.Key, ctrl, shift, alt);
        e.Handled = true;
    }

    // Helper function to check keyboard state
    private bool IsKeyPressed(VirtualKey key)
    {
        // In WinUI 3, this is the way to check for modifier keys
        var keyboardState = InputKeyboardSource.GetKeyStateForCurrentThread(key);
        return (keyboardState & CoreVirtualKeyStates.Down) == CoreVirtualKeyStates.Down;
    }

    protected override void OnNavigatedTo(NavigationEventArgs e)
    {
        base.OnNavigatedTo(e);
        
        // Use a simple but highly visible test markdown pattern
        string markdown = @"# Test Markdown Rendering

## This should be clearly visible

- List item 1
- List item 2
- List item 3

**Bold text** and *italic text* should render correctly.

```
Code block
test
```

> Quote block test

----

### More Test Content
Testing 1, 2, 3...";

        // Only use the parameter string if provided
        if (e.Parameter is string str && !string.IsNullOrEmpty(str))
        {
            markdown = str;
        }
        
        System.Diagnostics.Debug.WriteLine($"Setting up rendering with markdown content length: {markdown.Length}");
        D2DContext.SetupRendering(scpD2D, markdown);
    }

    protected override void OnNavigatedFrom(NavigationEventArgs e)
    {
        // Remove event handlers
        scpD2D.KeyDown -= ScpD2D_KeyDown;
        scpD2D.KeyUp -= ScpD2D_KeyUp;
        scpD2D.CharacterReceived -= ScpD2D_CharacterReceived;
        scpD2D.PointerPressed -= ScpD2D_PointerPressed;
        scpD2D.PointerReleased -= ScpD2D_PointerReleased;
        scpD2D.PointerMoved -= ScpD2D_PointerMoved;
        scpD2D.PointerWheelChanged -= ScpD2D_PointerWheelChanged;
        scpD2D.LostFocus -= ScpD2D_LostFocus;
        scpD2D.GotFocus -= ScpD2D_GotFocus;
        
        base.OnNavigatedFrom(e);
        D2DContext.UnloadPage();
    }
}
