using System.Runtime.InteropServices;
using System.Windows;
using System.Windows.Interop;

namespace BgiBridge.Bgi;

/// <summary>修复静默启动造成的 Win32/WPF 可见性不一致，不干预用户后续打开窗口。</summary>
public static class StartupVisibility
{
    [DllImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool IsWindowVisible(IntPtr window);

    /// <summary>在宿主 UI 线程调用一次；已经可见的窗口保持可见。</summary>
    public static void Normalize()
    {
        var window = Application.Current?.MainWindow;
        if (window is null) return;
        if (!window.IsLoaded)
        {
            void Loaded(object sender, RoutedEventArgs args)
            {
                window.Loaded -= Loaded;
                NormalizeWindow(window);
            }
            window.Loaded += Loaded;
            return;
        }
        NormalizeWindow(window);
    }

    private static void NormalizeWindow(Window window)
    {
        var handle = new WindowInteropHelper(window).Handle;
        if (handle != IntPtr.Zero && window.Visibility == Visibility.Visible && !IsWindowVisible(handle))
            // 与 BetterGI 托盘的 Hide() 使用同一 WPF 路径，绑定的 IsVisible 同步更新。
            window.Hide();
    }
}
