using Microsoft.UI.Xaml;
using System;

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
    public MainWindow()
    {
        this.InitializeComponent();
        this.Title = "Rust Native DOM in Win2D and WinUI";

        IntPtr hWndMain = WinRT.Interop.WindowNative.GetWindowHandle(this);
        this.Closed += MainWindow_Closed;
        D2DContext.Initialize(hWndMain);
    }

    private void MainWindow_Closed(object sender, WindowEventArgs args)
    {
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
