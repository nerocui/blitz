#pragma once  

#include "BlitzView.g.h"  
#include <d3d11.h> // Include Direct3D 11 headers  
#include <dxgi1_2.h> // Include DXGI 1.2 headers for IDXGISwapChain1  
#include <windows.ui.xaml.media.dxinterop.h>
#include <winrt/BlitzWinRT.h>

namespace winrt::Blitz_WinRTWrapper::implementation  
{  
   struct BlitzView : BlitzViewT<BlitzView>  
   {  
       BlitzView();  

       winrt::hstring Markdown() { return m_markdown; }  
       void Markdown(winrt::hstring value) { m_markdown = value; }  

   private:  
       void OnLoaded([[maybe_unused]] IInspectable const&,  
           [[maybe_unused]] Microsoft::UI::Xaml::RoutedEventArgs const&);  

       void OnRendering([[maybe_unused]] IInspectable const&,  
           [[maybe_unused]] IInspectable const&);  
       void LoadResources();  

       winrt::hstring m_markdown;  
       winrt::Microsoft::UI::Xaml::Controls::SwapChainPanel m_swapchainPanel{ nullptr };
	   BlitzWinRT::D2DRenderer m_d2dRenderer{ nullptr };
       com_ptr<ID3D11Device> m_device;  
       com_ptr<ID3D11DeviceContext> m_context;  
       com_ptr<IDXGISwapChain1> m_swapchain;  
   };  
}  

namespace winrt::Blitz_WinRTWrapper::factory_implementation  
{  
   struct BlitzView : BlitzViewT<BlitzView, implementation::BlitzView>  
   {  
   };  
}
