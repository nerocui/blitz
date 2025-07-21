# Blitz WinRT Package

A Windows Runtime (WinRT) wrapper for the Blitz HTML/CSS rendering engine, enabling HTML content rendering in Windows applications through SwapChainPanel controls.

## Overview

This package serves a similar role to `blitz-shell` but instead of rendering to full windows, it renders to SwapChainPanel controls that can be embedded in WinUI/UWP applications. This enables HTML/CSS content to be seamlessly integrated into modern Windows applications.

## Architecture

```
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│   WinUI/UWP     │    │   Blitz WinRT    │    │  Blitz Core     │
│   Application   │◄──►│     Package      │◄──►│   Engine        │
└─────────────────┘    └──────────────────┘    └─────────────────┘
         │                        │                       │
         │                        │                       ▼
    ┌────▼────┐              ┌────▼────┐            ┌──────────┐
    │SwapChain│              │Surface  │            │ Taffy    │
    │ Panel   │              │Manager  │            │ Layout   │
    └─────────┘              └─────────┘            └──────────┘
                                   │                       │
                                   ▼                       ▼
                             ┌──────────┐            ┌──────────┐
                             │ WGPU     │            │ Stylo    │
                             │ Surface  │            │ Styling  │
                             └──────────┘            └──────────┘
                                   │                       │
                                   ▼                       ▼
                             ┌──────────┐            ┌──────────┐
                             │AnyRender │            │ HTML5    │
                             │ Vello    │            │ Parser   │
                             └──────────┘            └──────────┘
```

## Key Components

### 1. BlitzView (WinRT Runtime Class)
- **Purpose**: Main entry point for WinUI/UWP applications
- **Interface**: Defined in `BlitzWinRT.idl`
- **Implementation**: `BlitzViewImpl` in `lib.rs`

### 2. Surface Manager (`surface_manager.rs`)
- **Purpose**: Handles WGPU surface creation from SwapChainPanel
- **Key Features**:
  - SwapChainPanel to WGPU surface conversion
  - DPI-aware surface management
  - Adapter/device initialization
  - Surface resizing and configuration

### 3. Event Conversion (`event_conversion.rs`)
- **Purpose**: Converts Windows messages to Blitz-compatible events
- **Supported Events**:
  - Mouse events (move, click, wheel)
  - Keyboard events (key press, release, character input)
  - Touch events (touch start, move, end)
  - Focus events (gained, lost)

### 4. View Implementation (`view_impl.rs`)
- **Purpose**: Core rendering logic and async task management
- **Key Features**:
  - HTML document management
  - Vello renderer integration
  - Async task processing
  - Viewport management

## Usage Example

### C# / WinUI Integration

```csharp
// In your WinUI application
public sealed partial class MainWindow : Window
{
    private BlitzView blitzView;
    
    public MainWindow()
    {
        this.InitializeComponent();
        InitializeBlitzView();
    }
    
    private void InitializeBlitzView()
    {
        // Get the SwapChainPanel from XAML
        var panel = MySwapChainPanel;
        
        // Create the BlitzView with markdown content
        var markdown = @"
# Welcome to Blitz WinRT!

This is **markdown** content being rendered in a WinUI application.

## Features
- Fast HTML/CSS rendering
- Dark/Light theme support
- Touch and mouse input
- GPU-accelerated graphics

```csharp
// Code syntax highlighting works too!
Console.WriteLine(""Hello from Blitz!"");
```
        ";
        
        // Create the BlitzView instance
        blitzView = new BlitzView((ulong)panel.NativePtr, markdown);
        
        // Set initial theme based on app theme
        blitzView.SetTheme(Application.Current.RequestedTheme == ApplicationTheme.Dark);
    }
    
    private void OnThemeChanged(object sender, RoutedEventArgs e)
    {
        // Update BlitzView theme when app theme changes
        bool isDark = Application.Current.RequestedTheme == ApplicationTheme.Dark;
        blitzView?.SetTheme(isDark);
    }
}
```

### XAML Integration

```xml
<Grid>
    <SwapChainPanel x:Name="MySwapChainPanel" 
                    Background="Transparent"/>
</Grid>
```

## Implementation Status

### ✅ Completed Components

1. **Project Structure**
   - Cargo.toml with proper dependencies
   - Module organization
   - IDL interface definition

2. **Surface Manager**
   - WGPU surface creation from SwapChainPanel
   - Device and adapter initialization
   - Surface configuration and resizing
   - DPI scaling support

3. **Event Conversion**
   - Windows message to Blitz event conversion
   - Mouse, keyboard, and touch event support
   - Modifier key tracking
   - Coordinate transformation

4. **View Implementation**
   - Core BlitzView implementation structure
   - Async task management system
   - HTML document lifecycle
   - Renderer integration points

5. **WinRT Integration**
   - COM/WinRT trait implementations
   - Thread safety for Windows Runtime
   - Factory pattern implementation
   - DLL export functions

### 🟡 Partially Implemented

1. **Rendering Pipeline**
   - Basic structure in place
   - Needs Vello scene building
   - Requires layout integration

2. **Markdown Processing**
   - Basic HTML wrapper implemented
   - Needs proper markdown parsing (pulldown-cmark)
   - Theme-aware styling in place

### ❌ Not Yet Implemented

1. **Full Rendering Integration**
   - Taffy layout calculation
   - Paint tree generation
   - Vello scene building and rendering

2. **Advanced Event Handling**
   - Event target resolution
   - DOM event dispatching
   - Input focus management

3. **Content Loading**
   - HTTP/HTTPS content loading
   - Asset management
   - Local file access

4. **Performance Optimizations**
   - Incremental rendering
   - Caching strategies
   - Memory management

## Development Notes

### Building

The package includes a build script that generates WinRT bindings from the IDL file. Currently requires:
- Windows SDK with `midlrt.exe`
- windows-bindgen for Rust binding generation

### Threading Model

- WinRT methods run on the UI thread
- Core rendering logic runs on async tasks
- Thread-safe communication via channels
- WGPU operations on dedicated thread

### Memory Management

- Arc/Mutex for shared state
- RAII for resource cleanup
- Careful pointer management for WinRT interop

## Future Enhancements

1. **Advanced Rendering Features**
   - CSS animations and transitions
   - WebGL-like 3D graphics
   - SVG rendering improvements

2. **Input Enhancements**
   - Gesture recognition
   - Accessibility support
   - IME integration

3. **Performance**
   - Multi-threaded rendering
   - Texture atlasing
   - Draw call batching

4. **Developer Experience**
   - Hot reload for development
   - Debugging tools
   - Performance profiling

## Contributing

When working on this package:
1. Follow Rust naming conventions (except for WinRT interfaces)
2. Add comprehensive documentation for public APIs
3. Include unit tests for core functionality
4. Use `#[allow(non_snake_case)]` for WinRT method names
5. Ensure thread safety for all shared state
