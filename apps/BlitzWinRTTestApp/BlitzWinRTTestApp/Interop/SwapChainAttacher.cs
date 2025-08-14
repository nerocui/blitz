using System;
using System.Runtime.InteropServices;
using System.Diagnostics;
using System.Reflection;
// Using the WinRT projection root namespace (types resolved via CsWinRT projection)
using BlitzWinUI;
using Microsoft.UI.Xaml.Controls;
using WinRT; // Add WinRT interop

namespace BlitzWinRTTestApp.Interop
{
    // Implements BlitzWinUI.ISwapChainAttacher so Rust can hand us a swapchain pointer.
    public sealed class SwapChainAttacher : BlitzWinUI.ISwapChainAttacher
    {
        private readonly SwapChainPanel _panel;
        private bool _isAttached = false;
    private ulong _pendingSwapchainPtr = 0;
    private bool _loadedHandlerAttached = false;

        public SwapChainAttacher(SwapChainPanel panel)
        {
            _panel = panel;
            Debug.WriteLine($"SwapChainAttacher: Created with panel {panel}");
            
            // Ensure the panel stays in the visual tree
            _panel.Unloaded += OnPanelUnloaded;
            // Defer attach until Loaded if not yet loaded
            if (!_panel.IsLoaded)
            {
                Debug.WriteLine("SwapChainAttacher: Panel not yet loaded; will defer swapchain attachment until Loaded event");
                _panel.Loaded += OnPanelLoaded;
                _loadedHandlerAttached = true;
            }
            
            // Basic projection check
            Debug.WriteLine($"Projection type check (ISwapChainAttacher) = { (this is BlitzWinUI.ISwapChainAttacher) }");
        }
        
        private void OnPanelUnloaded(object sender, Microsoft.UI.Xaml.RoutedEventArgs e)
        {
            Debug.WriteLine("SwapChainAttacher: Panel unloaded event fired");
            
            // Clean up if needed
            _isAttached = false;
        }

        public bool TestAttacherConnection()
        {
            Debug.WriteLine("TestAttacherConnection: Connection test successful!");
            return true;
        }

        public void AttachSwapChain(ulong swapchainPtr)
        {
            Debug.WriteLine($"AttachSwapChain: Called with swapchainPtr 0x{swapchainPtr:X}");
            Debug.WriteLine($"Panel state at AttachSwapChain: IsLoaded={_panel.IsLoaded} ActualSize={_panel.ActualWidth}x{_panel.ActualHeight} Visibility={_panel.Visibility}");
            
            // Don't try to actually attach test values
            if (swapchainPtr == 0xFEEDFACECAFEBEEF)
            {
                Debug.WriteLine("AttachSwapChain: Detected test pointer, ignoring actual attachment");
                return;
            }
            
            if (swapchainPtr == 0)
            {
                Debug.WriteLine("AttachSwapChain: Received null swapchain pointer, returning");
                return;
            }

            if (_isAttached)
            {
                Debug.WriteLine("AttachSwapChain: SwapChain already attached, detaching first");
                // Consider detaching existing swapchain if needed
            }

            // If panel not yet loaded, defer actual native SetSwapChain until Loaded
            if (!_panel.IsLoaded)
            {
                Debug.WriteLine("AttachSwapChain: Panel not loaded; deferring actual native SetSwapChain");
                _pendingSwapchainPtr = swapchainPtr;
                if (!_loadedHandlerAttached)
                {
                    _panel.Loaded += OnPanelLoaded;
                    _loadedHandlerAttached = true;
                }
                return;
            }

            TryAttachNow(swapchainPtr);
        }

        private void OnPanelLoaded(object sender, Microsoft.UI.Xaml.RoutedEventArgs e)
        {
            Debug.WriteLine("SwapChainAttacher: Panel Loaded event");
            if (_pendingSwapchainPtr != 0 && !_isAttached)
            {
                var ptr = _pendingSwapchainPtr;
                _pendingSwapchainPtr = 0;
                TryAttachNow(ptr);
            }
            if (_loadedHandlerAttached)
            {
                _panel.Loaded -= OnPanelLoaded; // one-shot
                _loadedHandlerAttached = false;
            }
        }

        private void TryAttachNow(ulong swapchainPtr)
        {
            try
            {
                TryAttachSwapChainWithMultipleApproaches(swapchainPtr);
            }
            catch (Exception ex)
            {
                Debug.WriteLine($"AttachSwapChain(TryAttachNow): All approaches failed. Final exception: {ex.GetType().Name}: {ex.Message}");
                Debug.WriteLine($"Stack trace: {ex.StackTrace}");
                Debug.WriteLine("AttachSwapChain(TryAttachNow): Will NOT crash app; rendering may remain blank.");
            }
        }
        
        private void TryAttachSwapChainWithMultipleApproaches(ulong swapchainPtr)
        {
            // Get the native panel interface
            var nativePanel = _panel.As<ISwapChainPanelNative>();
            Debug.WriteLine($"TryAttachSwapChainWithMultipleApproaches: Got native panel: {nativePanel}");
            
            // Log the panel properties for debugging
            Debug.WriteLine($"Panel dimensions: {_panel.ActualWidth}x{_panel.ActualHeight}");
            Debug.WriteLine($"Panel visibility: {_panel.Visibility}");
            Debug.WriteLine($"Panel loaded: {_panel.IsLoaded}");
            
            // Try approach 1: Direct pointer cast
            try
            {
                Debug.WriteLine("Approach 1: Direct pointer cast");
                var swapChainPtr = new IntPtr(unchecked((long)swapchainPtr));
                Debug.WriteLine($"Converted to IntPtr 0x{swapChainPtr.ToInt64():X}");
                
                // Verify it's not null after conversion
                if (swapChainPtr == IntPtr.Zero)
                {
                    Debug.WriteLine("Approach 1: Pointer became null after conversion");
                    throw new InvalidOperationException("Pointer became null after conversion");
                }
                
                nativePanel.SetSwapChain(swapChainPtr);
                Debug.WriteLine("Approach 1: Successfully attached swap chain");
                _isAttached = true;
                return; // Success!
            }
            catch (Exception ex)
            {
                Debug.WriteLine($"Approach 1 failed: {ex.GetType().Name}: {ex.Message}");
                if (ex.InnerException != null)
                {
                    Debug.WriteLine($"Inner exception: {ex.InnerException.GetType().Name}: {ex.InnerException.Message}");
                }
            }
            
            // Try approach 2: Add reference before using
            try
            {
                Debug.WriteLine("Approach 2: Add reference before using");
                var swapChainPtr = new IntPtr(unchecked((long)swapchainPtr));
                Marshal.AddRef(swapChainPtr);
                
                nativePanel.SetSwapChain(swapChainPtr);
                Debug.WriteLine("Approach 2: Successfully attached swap chain");
                _isAttached = true;
                return; // Success!
            }
            catch (Exception ex)
            {
                Debug.WriteLine($"Approach 2 failed: {ex.GetType().Name}: {ex.Message}");
            }
            
            // Try approach 3: Using GetTypedObjectForIUnknown to get a proper COM object
            try
            {
                Debug.WriteLine("Approach 3: Using GetTypedObjectForIUnknown");
                var swapChainPtr = new IntPtr(unchecked((long)swapchainPtr));
                var comObj = Marshal.GetObjectForIUnknown(swapChainPtr);
                Debug.WriteLine($"Got COM object: {comObj}");
                
                // We'd need to convert this to a proper IDXGISwapChain if we have the type
                // For now, just try the direct approach again
                nativePanel.SetSwapChain(swapChainPtr);
                Debug.WriteLine("Approach 3: Successfully attached swap chain");
                _isAttached = true;
                return; // Success!
            }
            catch (Exception ex)
            {
                Debug.WriteLine($"Approach 3 failed: {ex.GetType().Name}: {ex.Message}");
            }
            
            throw new InvalidOperationException("All approaches to attach swap chain failed");
        }
    }

    // Minimal COM interop for SwapChainPanel native interface
    [ComImport]
    [Guid("63aad0b8-7c24-40ff-85a8-640d944cc325")] // ISwapChainPanelNative
    [InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    internal interface ISwapChainPanelNative
    {
        // HRESULT SetSwapChain(IDXGISwapChain* pSwapChain)
        void SetSwapChain(IntPtr swapChain);
    }
}
