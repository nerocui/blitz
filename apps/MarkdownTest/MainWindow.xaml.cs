using Microsoft.UI.Xaml;
using System;
using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Composition.SystemBackdrops;
using Microsoft.UI.Windowing;
using Microsoft.Windows.AppLifecycle;
using Windows.ApplicationModel.Activation;

// To learn more about WinUI, the WinUI project structure,
// and more about our project templates, see: http://aka.ms/winui-project-info.

namespace MarkdownTest;

/// <summary>
/// An empty window that can be used on its own or navigated to within a Frame.
/// </summary>
public sealed partial class MainWindow : Window
{
    private string _markdown1 = """
# Markdown to HTML Conversion

---

## Overview

This document demonstrates the capability of converting markdown into HTML and rendering it in a native DOM. The rendering pipeline utilizes the **Rust programming language**, leveraging the **Direct2D API** for graphical drawing. The result is packaged as a **WinRT component** and consumed seamlessly within a **C# WinUI application**.

---

## Features

### Core Components
1. **Markdown to HTML Converter**:
   - Parses markdown syntax and generates HTML output.
   - Supports nested elements and complex formatting.

2. **Native DOM Renderer**:
   - Written entirely in Rust for performance and efficiency.
   - Capable of dynamically updating and manipulating the DOM structure.

3. **Direct2D Integration**:
   - Renders graphical elements such as text, tables, and decorations.
   - Ensures high-quality rendering with anti-aliasing and hardware acceleration.

4. **WinRT Packaging**:
   - Provides interoperability between the Rust implementation and Windows Runtime.
   - Enables the usage of Rust components in C# projects.

5. **WinUI Consumption**:
   - Embeds the rendered content within a C# WinUI application.
   - Uses XAML for UI layout and integrates seamlessly with WinUI controls.

---

## Markdown Syntax Examples

### Heading Levels

```markdown
# Heading Level 1
## Heading Level 2
### Heading Level 3

""";

    private string _markdown2 = """
| Feature       | Description                                    | Status        |
|---------------|------------------------------------------------|---------------|
| Markdown      | Parsed into HTML elements                     | Completed     |
| Rendering     | DOM drawn using Direct2D API                  | In Progress   |
| WinRT Package | Allows integration with C# WinUI applications | Completed     |

""";
    
    private DispatcherQueueTimer _themeChangeTimer;
    private ElementTheme _currentTheme = ElementTheme.Default;
    private Microsoft.UI.Xaml.Controls.Frame _contentFrame;

    public MainWindow()
    {
        this.InitializeComponent();
        this.Title = "Rust Native DOM in Win2D and WinUI";

        // Get native window handle for D2D initialization
        IntPtr hWndMain = WinRT.Interop.WindowNative.GetWindowHandle(this);
        
        // Setup event handlers
        this.Closed += MainWindow_Closed;
        
        // Get the content frame reference
        _contentFrame = contentFrame;
        _contentFrame.ActualThemeChanged += ContentFrame_ActualThemeChanged;
        _currentTheme = _contentFrame.ActualTheme;
        
        // Create a timer for debouncing theme changes
        _themeChangeTimer = DispatcherQueue.CreateTimer();
        _themeChangeTimer.Interval = TimeSpan.FromMilliseconds(100);
        _themeChangeTimer.Tick += ThemeChangeTimer_Tick;
        
        // Initialize D2D
        D2DContext.Initialize(hWndMain);
        
        // Register for app lifecycle events with WinAppSDK
        // In WinUI 3, we need to handle lifecycle events differently
        App.Current.UnhandledException += Current_UnhandledException;

        // Unfortunately, WinUI 3 doesn't have direct Suspending/Resuming events
        // We'll use our own handling for window activation/deactivation as a proxy
        this.Activated += MainWindow_Activated;
    }

    private void MainWindow_Activated(object sender, WindowActivatedEventArgs args)
    {
        // Use activation state as a proxy for app resume/suspend
        if (args.WindowActivationState != WindowActivationState.Deactivated)
        {
            // Window activated - similar to app resume
            D2DContext.Resume();
        }
        else
        {
            // Window deactivated - similar to app suspend
            D2DContext.Suspend();
        }
    }

    private void Current_UnhandledException(object sender, Microsoft.UI.Xaml.UnhandledExceptionEventArgs e)
    {
        // Handle any unhandled exceptions
        e.Handled = true;
    }

    private void ThemeChangeTimer_Tick(DispatcherQueueTimer sender, object args)
    {
        _themeChangeTimer.Stop();
        
        // Update theme in D2D renderer
        D2DContext.SetTheme(_currentTheme == ElementTheme.Dark);
    }

    private void ContentFrame_ActualThemeChanged(FrameworkElement sender, object args)
    {
        _currentTheme = _contentFrame.ActualTheme;
        
        // Use timer to avoid multiple calls in short succession
        _themeChangeTimer.Start();
    }

    private void MainWindow_Closed(object sender, WindowEventArgs args)
    {
        // Clean up event handlers
        if (_contentFrame != null)
            _contentFrame.ActualThemeChanged -= ContentFrame_ActualThemeChanged;
        
        this.Activated -= MainWindow_Activated;
        App.Current.UnhandledException -= Current_UnhandledException;
        
        // Clean up D2D resources
        D2DContext.Clean();
    }

    private void NavigationView_SelectionChanged(Microsoft.UI.Xaml.Controls.NavigationView sender, Microsoft.UI.Xaml.Controls.NavigationViewSelectionChangedEventArgs args)
    {
        var item = sender.SelectedItem as Microsoft.UI.Xaml.Controls.NavigationViewItem;
        if (item != null)
        {
            var tag = item.Tag.ToString();
            if (tag == "SamplePage1")
            {
                contentFrame.Navigate(typeof(MarkdownPage), _markdown1);
            } else if (tag == "SamplePage2")
            {
                contentFrame.Navigate(typeof(MarkdownPage), _markdown2);
            }
        }
    }
}
