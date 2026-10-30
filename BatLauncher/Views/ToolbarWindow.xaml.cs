using System.Diagnostics;
using System.IO;
using System.Runtime.InteropServices;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using System.Windows.Interop;
using BatLauncher.Models;
using BatLauncher.Services;
using BatLauncher.ViewModels;

namespace BatLauncher.Views;

public partial class ToolbarWindow : Window
{
    private readonly MainViewModel _vm;
    private MainWindow? _managementWindow;
    private bool _isShowingDialog;

    [DllImport("kernel32.dll")]
    private static extern bool SetProcessWorkingSetSize(nint process, nint min, nint max);

    [DllImport("user32.dll")]
    private static extern bool GetCursorPos(out POINT point);

    [DllImport("user32.dll")]
    private static extern nint MonitorFromPoint(POINT pt, uint flags);

    [DllImport("user32.dll")]
    private static extern bool GetMonitorInfo(nint hMonitor, ref MONITORINFO info);

    [StructLayout(LayoutKind.Sequential)]
    private struct POINT { public int X, Y; }

    [StructLayout(LayoutKind.Sequential)]
    private struct RECT { public int Left, Top, Right, Bottom; }

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Auto)]
    private struct MONITORINFO
    {
        public int cbSize;
        public RECT rcMonitor;
        public RECT rcWork;
        public uint dwFlags;
    }

    public ToolbarWindow(MainViewModel vm)
    {
        InitializeComponent();
        _vm = vm;
        DataContext = _vm;

        Loaded += OnLoaded;
        StateChanged += OnStateChanged;
        Deactivated += OnDeactivated;
    }

    private void OnDeactivated(object? sender, EventArgs e)
    {
        if (_isShowingDialog) return;
        // Hide to tray instead of just minimizing
        ((App)Application.Current).HideToolbar();
    }

    private void OnLoaded(object sender, RoutedEventArgs e)
    {
        PositionAboveTaskbar();
        UpdateSeparatorVisibility();
        _vm.AllEntries.CollectionChanged += (_, _) => UpdateSeparatorVisibility();

        // Trim working set after everything is loaded
        Dispatcher.BeginInvoke(System.Windows.Threading.DispatcherPriority.ApplicationIdle, () =>
        {
            GC.Collect(2, GCCollectionMode.Aggressive, true, true);
            GC.WaitForPendingFinalizers();
            SetProcessWorkingSetSize(Process.GetCurrentProcess().Handle, -1, -1);
        });
    }

    private void OnStateChanged(object? sender, EventArgs e)
    {
        if (WindowState == WindowState.Normal)
        {
            Topmost = true;
            PositionAboveTaskbar();
        }
    }

    public void PositionAboveTaskbar()
    {
        UpdateLayout();

        // Get the monitor where the cursor currently is
        GetCursorPos(out var cursor);
        var hMonitor = MonitorFromPoint(cursor, 2 /* MONITOR_DEFAULTTONEAREST */);
        var info = new MONITORINFO { cbSize = Marshal.SizeOf<MONITORINFO>() };

        if (GetMonitorInfo(hMonitor, ref info))
        {
            // Use PresentationSource to convert physical pixels to WPF DIPs
            var source = PresentationSource.FromVisual(this);
            var dpiX = source?.CompositionTarget?.TransformFromDevice.M11 ?? 1.0;
            var dpiY = source?.CompositionTarget?.TransformFromDevice.M22 ?? 1.0;

            var workLeft = info.rcWork.Left * dpiX;
            var workTop = info.rcWork.Top * dpiY;
            var workWidth = (info.rcWork.Right - info.rcWork.Left) * dpiX;
            var workBottom = info.rcWork.Bottom * dpiY;

            Left = workLeft + (workWidth - ActualWidth) / 2;
            Top = workBottom - ActualHeight - 4;
        }
        else
        {
            // Fallback to primary monitor
            var workArea = SystemParameters.WorkArea;
            Left = workArea.Left + (workArea.Width - ActualWidth) / 2;
            Top = workArea.Bottom - ActualHeight - 4;
        }
    }

    private void UpdateSeparatorVisibility()
    {
        Separator1.Visibility = _vm.AllEntries.Count > 0
            ? Visibility.Visible
            : Visibility.Collapsed;
    }

    private void Bar_MouseLeftButtonDown(object sender, MouseButtonEventArgs e)
    {
        if (e.ClickCount == 1)
            DragMove();
    }

    private void ScrollViewer_PreviewMouseWheel(object sender, MouseWheelEventArgs e)
    {
        if (sender is ScrollViewer sv)
        {
            sv.ScrollToHorizontalOffset(sv.HorizontalOffset - e.Delta);
            e.Handled = true;
        }
    }

    private void ScriptIcon_Click(object sender, RoutedEventArgs e)
    {
        if (sender is Button btn && btn.Tag is BatEntryViewModel entry)
        {
            RunScript(entry);
        }
    }

    private void RunScript(BatEntryViewModel entry)
    {
        if (!File.Exists(entry.FilePath))
        {
            MessageBox.Show($"File not found:\n{entry.FilePath}", "Error",
                MessageBoxButton.OK, MessageBoxImage.Error);
            return;
        }

        try
        {
            var ext = Path.GetExtension(entry.FilePath).ToLowerInvariant();
            var psi = ext is ".bat" or ".cmd"
                ? new ProcessStartInfo
                {
                    FileName = "cmd.exe",
                    Arguments = $"/c \"{entry.FilePath}\"",
                    WorkingDirectory = Path.GetDirectoryName(entry.FilePath) ?? "",
                    UseShellExecute = true
                }
                : new ProcessStartInfo
                {
                    FileName = entry.FilePath,
                    WorkingDirectory = Path.GetDirectoryName(entry.FilePath) ?? "",
                    UseShellExecute = true
                };
            Process.Start(psi);
            // Hide to tray after launching script (stay running in background)
            ((App)Application.Current).HideToolbar();
        }
        catch (Exception ex)
        {
            MessageBox.Show($"Error:\n{ex.Message}", "Error",
                MessageBoxButton.OK, MessageBoxImage.Error);
        }
    }

    private void Add_Click(object sender, RoutedEventArgs e)
    {
        _isShowingDialog = true;
        var entry = new BatEntry();
        var dlg = new EditDialog(entry, true);
        if (dlg.ShowDialog() == true && dlg.Result != null)
        {
            _vm.AddEntryDirect(dlg.Result);
        }
        _isShowingDialog = false;
    }

    private void Manage_Click(object sender, RoutedEventArgs e)
    {
        _isShowingDialog = true;
        if (_managementWindow == null || !_managementWindow.IsLoaded)
        {
            _managementWindow = new MainWindow(_vm);
            _managementWindow.Closed += (_, _) => _managementWindow = null;
            _managementWindow.Show();
        }
        else
        {
            _managementWindow.Activate();
            if (_managementWindow.WindowState == WindowState.Minimized)
                _managementWindow.WindowState = WindowState.Normal;
        }
        _isShowingDialog = false;
    }

    private void Close_Click(object sender, RoutedEventArgs e)
    {
        _managementWindow?.Close();
        // Hide to tray instead of closing
        ((App)Application.Current).HideToolbar();
    }
}
