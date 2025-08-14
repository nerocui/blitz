#include "pch.h"
#include "BlitzView.h"
#if __has_include("BlitzView.g.cpp")
#include "BlitzView.g.cpp"
#endif

using namespace winrt;
using namespace winrt::Microsoft::UI::Xaml;
using namespace winrt::Microsoft::UI::Xaml::Controls;
using namespace winrt::Microsoft::UI::Xaml::Input;
using namespace winrt::Microsoft::UI::Xaml::Media;

namespace winrt::Blitz::implementation
{
    BlitzView::BlitzView()
    {
        DefaultStyleKey(box_value(L"Blitz.BlitzView"));
    }

    // Called after template applied; override to grab named parts.
    void BlitzView::InitializeHostIfReady()
    {
        if (!m_panel)
        {
            // Look up template part
            if (auto root = this->GetTemplateChild(L"swapChainPanel"))
            {
                try { m_panel = root.as<SwapChainPanel>(); }
                catch (...) { }
            }
        }
        if (m_host || !m_panel)
        {
            return; // Already initialized or missing panel
        }

        // Create attacher using panel
        try
        {
            m_attacher = winrt::Blitz::Attacher(m_panel);
        }
        catch (...) { m_attacher = nullptr; }
        if (!m_attacher)
        {
            return;
        }

        // Determine initial size/scale
        float scale = 1.0f;
        if (auto xr = this->XamlRoot())
        {
            scale = static_cast<float>(xr.RasterizationScale());
        }
        uint32_t width = std::max<uint32_t>(1, static_cast<uint32_t>(m_panel.ActualWidth()));
        uint32_t height = std::max<uint32_t>(1, static_cast<uint32_t>(m_panel.ActualHeight()));

        // Provide basic HTML (could later be a dependency property)
        hstring initialHtml = L"<html><body style='background:#202020;color:#EEE;font-family:sans-serif'>Blitz host</body></html>";
        try
        {
            m_host = winrt::BlitzWinUI::Host(m_attacher, width, height, scale, initialHtml);
        }
        catch (...)
        {
            m_host = nullptr;
            return;
        }

        EnsureRenderLoop();
    }

    void BlitzView::EnsureRenderLoop()
    {
        if (m_renderLoopAttached)
            return;
        m_renderingToken = CompositionTarget::Rendering({ this, &BlitzView::OnRendering });
        m_renderLoopAttached = true;
    }

    void BlitzView::StopRenderLoop()
    {
        if (!m_renderLoopAttached)
            return;
        CompositionTarget::Rendering(m_renderingToken);
        m_renderLoopAttached = false;
    }

    void BlitzView::OnRendering(winrt::Windows::Foundation::IInspectable const&, winrt::Windows::Foundation::IInspectable const&)
    {
        if (m_host)
        {
            try { m_host.RenderOnce(); }
            catch (...) { /* stop loop on persistent failure? */ }
        }
    }

    void BlitzView::OnPanelLoaded(winrt::Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        InitializeHostIfReady();
    }

    void BlitzView::OnPanelSizeChanged(winrt::Windows::Foundation::IInspectable const&, SizeChangedEventArgs const&)
    {
        ForwardResize();
    }

    void BlitzView::ForwardResize()
    {
        if (!m_host || !m_panel)
            return;
        float scale = 1.0f;
        if (auto xr = this->XamlRoot())
        {
            scale = static_cast<float>(xr.RasterizationScale());
        }
        uint32_t width = std::max<uint32_t>(1, static_cast<uint32_t>(m_panel.ActualWidth()));
        uint32_t height = std::max<uint32_t>(1, static_cast<uint32_t>(m_panel.ActualHeight()));
        try { m_host.Resize(width, height, scale); }
        catch (...) { }
    }

    void BlitzView::PanelPointerMoved(winrt::Windows::Foundation::IInspectable const&, PointerRoutedEventArgs const& e)
    {
        if (!m_host || !m_panel) return;
        auto pt = e.GetCurrentPoint(m_panel);
        uint32_t modifiers = (uint32_t)e.KeyModifiers();
        uint32_t buttons = 0;
        if (pt.Properties().IsLeftButtonPressed()) buttons |= 1;
        if (pt.Properties().IsRightButtonPressed()) buttons |= 2;
        if (pt.Properties().IsMiddleButtonPressed()) buttons |= 4;
        if (pt.Properties().IsXButton1Pressed()) buttons |= 8;
        if (pt.Properties().IsXButton2Pressed()) buttons |= 16;
        try { m_host.PointerMove((float)pt.Position().X, (float)pt.Position().Y, buttons, modifiers); } catch (...) {}
    }

    void BlitzView::PanelPointerPressed(winrt::Windows::Foundation::IInspectable const&, PointerRoutedEventArgs const& e)
    {
        if (!m_host || !m_panel) return;
        auto pt = e.GetCurrentPoint(m_panel);
        uint8_t button = 0;
        if (pt.Properties().IsRightButtonPressed()) button = 2;
        else if (pt.Properties().IsMiddleButtonPressed()) button = 1;
        uint32_t buttons = 0;
        if (pt.Properties().IsLeftButtonPressed()) buttons |= 1;
        if (pt.Properties().IsRightButtonPressed()) buttons |= 2;
        if (pt.Properties().IsMiddleButtonPressed()) buttons |= 4;
        if (pt.Properties().IsXButton1Pressed()) buttons |= 8;
        if (pt.Properties().IsXButton2Pressed()) buttons |= 16;
        uint32_t modifiers = (uint32_t)e.KeyModifiers();
        try { m_host.PointerDown((float)pt.Position().X, (float)pt.Position().Y, button, buttons, modifiers); } catch (...) {}
    }

    void BlitzView::PanelPointerReleased(winrt::Windows::Foundation::IInspectable const&, PointerRoutedEventArgs const& e)
    {
        if (!m_host || !m_panel) return;
        auto pt = e.GetCurrentPoint(m_panel);
        uint8_t button = 0; // heuristic: left release maps to 0
        uint32_t modifiers = (uint32_t)e.KeyModifiers();
        try { m_host.PointerUp((float)pt.Position().X, (float)pt.Position().Y, button, 0, modifiers); } catch (...) {}
    }

    void BlitzView::PanelPointerWheelChanged(winrt::Windows::Foundation::IInspectable const&, PointerRoutedEventArgs const& e)
    {
        if (!m_host || !m_panel) return;
        auto pt = e.GetCurrentPoint(m_panel);
        int raw = pt.Properties().MouseWheelDelta(); // multiples of 120
        double linesPerNotch = 1.0;
        double pixelsPerLine = 48.0;
        double dy = raw / 120.0 * linesPerNotch * pixelsPerLine;
        double dx = 0.0;
        if ((e.KeyModifiers() & Windows::System::VirtualKeyModifiers::Shift) == Windows::System::VirtualKeyModifiers::Shift)
        {
            dx = dy; dy = 0.0;
        }
        try { m_host.WheelScroll(dx, dy); } catch (...) {}
        e.Handled(true);
    }

    // Override OnApplyTemplate to wire events after control template is applied
    void BlitzView::OnApplyTemplate()
    {
        // Call base Control implementation first
        //base_type::OnApplyTemplate();

        // Detach existing handlers if re-templated
        if (m_panel)
        {
            auto clear = [](auto& token, auto remover)
            {
                if (token.value != 0) { remover(token); token.value = 0; }
            };
            clear(m_loadedToken,        [this](auto const& t){ m_panel.Loaded(t); });
            clear(m_sizeChangedToken,   [this](auto const& t){ m_panel.SizeChanged(t); });
            clear(m_pointerMovedToken,  [this](auto const& t){ m_panel.PointerMoved(t); });
            clear(m_pointerPressedToken,[this](auto const& t){ m_panel.PointerPressed(t); });
            clear(m_pointerReleasedToken,[this](auto const& t){ m_panel.PointerReleased(t); });
            clear(m_pointerWheelChangedToken,[this](auto const& t){ m_panel.PointerWheelChanged(t); });
        }
        m_panel = nullptr;

        InitializeHostIfReady();
        if (m_panel)
        {
            m_loadedToken = m_panel.Loaded({ this, &BlitzView::OnPanelLoaded });
            m_sizeChangedToken = m_panel.SizeChanged({ this, &BlitzView::OnPanelSizeChanged });
            m_pointerMovedToken = m_panel.PointerMoved({ this, &BlitzView::PanelPointerMoved });
            m_pointerPressedToken = m_panel.PointerPressed({ this, &BlitzView::PanelPointerPressed });
            m_pointerReleasedToken = m_panel.PointerReleased({ this, &BlitzView::PanelPointerReleased });
            m_pointerWheelChangedToken = m_panel.PointerWheelChanged({ this, &BlitzView::PanelPointerWheelChanged });
        }
    }
}

// Override single-param versions; forward to panel versions
void winrt::Blitz::implementation::BlitzView::OnPointerMoved(PointerRoutedEventArgs const& e)
{ PanelPointerMoved(nullptr, e); }
void winrt::Blitz::implementation::BlitzView::OnPointerPressed(PointerRoutedEventArgs const& e)
{ PanelPointerPressed(nullptr, e); }
void winrt::Blitz::implementation::BlitzView::OnPointerReleased(PointerRoutedEventArgs const& e)
{ PanelPointerReleased(nullptr, e); }
void winrt::Blitz::implementation::BlitzView::OnPointerWheelChanged(PointerRoutedEventArgs const& e)
{ PanelPointerWheelChanged(nullptr, e); }
