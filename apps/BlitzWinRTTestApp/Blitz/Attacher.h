#pragma once

#include "Attacher.g.h"
#include <winrt/Microsoft.UI.Xaml.Controls.h>

namespace winrt::Blitz::implementation
{
    struct Attacher : AttacherT<Attacher>
    {
        Attacher(winrt::Windows::Foundation::IInspectable const& panel); // panel expected to be SwapChainPanel

        // BlitzWinUI.ISwapChainAttacher implementation
        void AttachSwapChain(uint64_t swapchainPtr);
        bool TestAttacherConnection();

    private:
        winrt::Microsoft::UI::Xaml::Controls::SwapChainPanel m_panel{ nullptr };
        uint64_t m_lastSwapchainPtr{ 0 };
    };
}

namespace winrt::Blitz::factory_implementation
{
    struct Attacher : AttacherT<Attacher, implementation::Attacher>
    {
    };
}
