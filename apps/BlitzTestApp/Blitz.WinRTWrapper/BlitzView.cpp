#include "pch.h"
#include "BlitzView.h"
#if __has_include("BlitzView.g.cpp")
#include "BlitzView.g.cpp"
#endif

using namespace winrt;
using namespace Microsoft::UI::Xaml;
using namespace Microsoft::UI::Xaml::Media;
using namespace Windows::ApplicationModel;
using namespace Windows::Storage;
using namespace Windows::Storage::Streams;
using namespace DirectX;

// To learn more about WinUI, the WinUI project structure,
// and more about our project templates, see: http://aka.ms/winui-project-info.

namespace winrt::Blitz_WinRTWrapper::implementation
{
    BlitzView::BlitzView()
    {
        DefaultStyleKey(winrt::box_value(L"Blitz_WinRTWrapper.BlitzView"));
		m_swapchainPanel = GetTemplateChild(L"BlitzSwapChain").try_as<winrt::Microsoft::UI::Xaml::Controls::SwapChainPanel>();
        Loaded({ this, &BlitzView::OnLoaded });
    }

    void BlitzView::OnLoaded([[maybe_unused]] IInspectable const&,
        [[maybe_unused]] RoutedEventArgs const&)
    {
        LoadResources();
            
        uint64_t contextPtr = reinterpret_cast<uint64_t>(m_context.get());
        m_d2dRenderer = BlitzWinRT::D2DRenderer(contextPtr);
        m_d2dRenderer.Render(L"# Hello From C++");
        CompositionTarget::Rendering({ this, &BlitzView::OnRendering });
    }

    void BlitzView::OnRendering([[maybe_unused]] IInspectable const&,
        [[maybe_unused]] IInspectable const&)
    {
		m_d2dRenderer.Tick();
    }

    void BlitzView::LoadResources()
    {
        const std::array<D3D_FEATURE_LEVEL, 4> feature_levels{
            D3D_FEATURE_LEVEL_12_1, D3D_FEATURE_LEVEL_12_0, D3D_FEATURE_LEVEL_11_1,
            D3D_FEATURE_LEVEL_11_0 };
        check_hresult(D3D11CreateDevice(
            nullptr, D3D_DRIVER_TYPE_HARDWARE, nullptr, D3D11_CREATE_DEVICE_DEBUG,
            feature_levels.data(), static_cast<UINT>(feature_levels.size()),
            D3D11_SDK_VERSION, m_device.put(), nullptr, m_context.put()));

        com_ptr<IDXGIFactory2> dxgi_factory{};
        check_hresult(CreateDXGIFactory2(DXGI_CREATE_FACTORY_DEBUG,
            IID_PPV_ARGS(dxgi_factory.put())));

        DXGI_SWAP_CHAIN_DESC1 swapchain_desc{};
        swapchain_desc.Width = static_cast<UINT>(m_swapchainPanel.ActualWidth());
        swapchain_desc.Height = static_cast<UINT>(m_swapchainPanel.ActualHeight());
        swapchain_desc.Format = DXGI_FORMAT_R8G8B8A8_UNORM;
        swapchain_desc.SampleDesc.Count = 1;
        swapchain_desc.SampleDesc.Quality = 0;
        swapchain_desc.BufferUsage = DXGI_USAGE_RENDER_TARGET_OUTPUT;
        swapchain_desc.BufferCount = 2;
        swapchain_desc.Scaling = DXGI_SCALING_STRETCH;
        swapchain_desc.SwapEffect = DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL;

        check_hresult(dxgi_factory->CreateSwapChainForComposition(
            m_device.get(), &swapchain_desc, nullptr, m_swapchain.put()));

        check_hresult(m_swapchainPanel.as<ISwapChainPanelNative>()->SetSwapChain(
            m_swapchain.get()));
    }
}
