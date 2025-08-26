#pragma once

#include "BlitzView.g.h"
#include <winrt/Microsoft.UI.Xaml.Controls.h>
#include <winrt/Microsoft.UI.Xaml.Input.h>
#include <winrt/Microsoft.UI.Xaml.Media.h>
#include <winrt/Windows.System.h>
#include <winrt/BlitzWinUI.h>
#include <winrt/Blitz.h> // Attacher runtimeclass (same project)

namespace winrt::Blitz::implementation
{
    struct BlitzView : BlitzViewT<BlitzView>
    {
        BlitzView();

        // Public (WinRT) methods we might expose later could go here.

    public:
        // Control overrides / UIElement overrides (must be public)
        void OnApplyTemplate();
        void OnPointerMoved(winrt::Microsoft::UI::Xaml::Input::PointerRoutedEventArgs const&);
        void OnPointerPressed(winrt::Microsoft::UI::Xaml::Input::PointerRoutedEventArgs const&);
        void OnPointerReleased(winrt::Microsoft::UI::Xaml::Input::PointerRoutedEventArgs const&);
        void OnPointerWheelChanged(winrt::Microsoft::UI::Xaml::Input::PointerRoutedEventArgs const&);
        winrt::hstring HTML() const; // Property getter
        void HTML(winrt::hstring const& value); // Property setter
    bool DebugOverlayEnabled() const;
    void DebugOverlayEnabled(bool value);

    private:
        // Lifecycle
        void InitializeHostIfReady();
        void EnsureRenderLoop();
        void StopRenderLoop();

        // Event handlers
        void OnPanelLoaded(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void OnPanelSizeChanged(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::SizeChangedEventArgs const&);
        // Panel subscription handlers (internal wiring)
        void PanelPointerMoved(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::Input::PointerRoutedEventArgs const&);
        void PanelPointerPressed(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::Input::PointerRoutedEventArgs const&);
        void PanelPointerReleased(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::Input::PointerRoutedEventArgs const&);
        void PanelPointerWheelChanged(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::Input::PointerRoutedEventArgs const&);
        void OnRendering(winrt::Windows::Foundation::IInspectable const&, winrt::Windows::Foundation::IInspectable const&);
    void OnXamlRootChanged(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::XamlRootChangedEventArgs const&);

        // Helpers
        void ForwardResize();

        // State
        winrt::Microsoft::UI::Xaml::Controls::SwapChainPanel m_panel{ nullptr };
        Attacher m_attacher{ nullptr };
        winrt::BlitzWinUI::Host m_host{ nullptr };
    // Network fetcher (host-driven HTTP); created after host and injected via SetNetworkFetcher.
    winrt::Blitz::NetworkFetcher m_fetcher{ nullptr };
        bool m_renderLoopAttached{ false };
        winrt::hstring m_html; // backing for HTML property
    bool m_debugOverlayEnabled{ false }; // backing for DebugOverlayEnabled property

        // Event tokens for cleanup (not strictly necessary yet)
        winrt::event_token m_loadedToken{};
        winrt::event_token m_sizeChangedToken{};
        winrt::event_token m_pointerMovedToken{};
        winrt::event_token m_pointerPressedToken{};
        winrt::event_token m_pointerReleasedToken{};
        winrt::event_token m_pointerWheelChangedToken{};
        winrt::event_token m_renderingToken{};
    winrt::event_token m_xamlRootChangedToken{};
    };
}

namespace winrt::Blitz::factory_implementation
{
    struct BlitzView : BlitzViewT<BlitzView, implementation::BlitzView>
    {
    };
}
