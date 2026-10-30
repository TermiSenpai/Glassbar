using System.Drawing;
using System.IO;
using System.Windows;
using System.Windows.Threading;
using BatLauncher.ViewModels;
using BatLauncher.Views;
using Microsoft.Win32;
using Forms = System.Windows.Forms;

namespace BatLauncher;

public partial class App : Application
{
    private Forms.NotifyIcon? _trayIcon;
    private ToolbarWindow? _toolbar;
    private static Mutex? _mutex;
    private EventWaitHandle? _showSignal;
    private Thread? _signalThread;

    private const string MutexName = "BatLauncher_SingleInstance";
    private const string SignalName = "BatLauncher_ShowSignal";
    private const string RegistryKey = @"SOFTWARE\Microsoft\Windows\CurrentVersion\Run";
    private const string AppName = "BatLauncher";

    protected override void OnStartup(StartupEventArgs e)
    {
        // Single instance check
        _mutex = new Mutex(true, MutexName, out bool isNew);
        if (!isNew)
        {
            _mutex.Dispose();
            _mutex = null;
            // Signal the existing instance to show itself
            try
            {
                var signal = EventWaitHandle.OpenExisting(SignalName);
                signal.Set();
                signal.Dispose();
            }
            catch { }
            Shutdown();
            return;
        }

        base.OnStartup(e);

        // Listen for signals from other instances
        _showSignal = new EventWaitHandle(false, EventResetMode.AutoReset, SignalName);
        _signalThread = new Thread(() =>
        {
            while (_showSignal.WaitOne())
            {
                Dispatcher.BeginInvoke(ShowToolbar);
            }
        }) { IsBackground = true };
        _signalThread.Start();

        var vm = new MainViewModel();
        _toolbar = new ToolbarWindow(vm);
        MainWindow = _toolbar;

        SetupTrayIcon();

        _toolbar.Show();

        // Load data after the window is already visible
        Dispatcher.InvokeAsync(() => vm.LoadData(), DispatcherPriority.Background);
    }

    private void SetupTrayIcon()
    {
        _trayIcon = new Forms.NotifyIcon
        {
            Text = "BatLauncher",
            Visible = true
        };

        // Load icon from embedded resource
        var iconUri = Path.Combine(AppContext.BaseDirectory, "Resources", "app.ico");
        if (File.Exists(iconUri))
        {
            _trayIcon.Icon = new Icon(iconUri);
        }
        else
        {
            // Fallback: extract from assembly
            _trayIcon.Icon = Icon.ExtractAssociatedIcon(Environment.ProcessPath!);
        }

        // Context menu
        var menu = new Forms.ContextMenuStrip();

        var showItem = new Forms.ToolStripMenuItem("Abrir BatLauncher");
        showItem.Click += (_, _) => ShowToolbar();
        showItem.Font = new Font(showItem.Font, System.Drawing.FontStyle.Bold);
        menu.Items.Add(showItem);

        menu.Items.Add(new Forms.ToolStripSeparator());

        var startupItem = new Forms.ToolStripMenuItem("Iniciar con Windows");
        startupItem.Checked = IsStartupEnabled();
        startupItem.Click += (_, _) =>
        {
            ToggleStartup();
            startupItem.Checked = IsStartupEnabled();
        };
        menu.Items.Add(startupItem);

        menu.Items.Add(new Forms.ToolStripSeparator());

        var exitItem = new Forms.ToolStripMenuItem("Salir");
        exitItem.Click += (_, _) => ExitApp();
        menu.Items.Add(exitItem);

        _trayIcon.ContextMenuStrip = menu;
        _trayIcon.DoubleClick += (_, _) => ShowToolbar();
        _trayIcon.Click += (_, e2) =>
        {
            if (e2 is Forms.MouseEventArgs me && me.Button == Forms.MouseButtons.Left)
                ShowToolbar();
        };
    }

    public void ShowToolbar()
    {
        if (_toolbar == null) return;

        _toolbar.Show();
        _toolbar.WindowState = WindowState.Normal;
        _toolbar.PositionAboveTaskbar();
        _toolbar.Activate();
    }

    public void HideToolbar()
    {
        _toolbar?.Hide();
    }

    public void ExitApp()
    {
        if (_trayIcon != null)
        {
            _trayIcon.Visible = false;
            _trayIcon.Dispose();
            _trayIcon = null;
        }

        _mutex?.ReleaseMutex();
        _mutex?.Dispose();
        _mutex = null;

        Shutdown();
    }

    public static bool IsStartupEnabled()
    {
        using var key = Registry.CurrentUser.OpenSubKey(RegistryKey, false);
        return key?.GetValue(AppName) != null;
    }

    public static void ToggleStartup()
    {
        using var key = Registry.CurrentUser.OpenSubKey(RegistryKey, true);
        if (key == null) return;

        if (IsStartupEnabled())
        {
            key.DeleteValue(AppName, false);
        }
        else
        {
            var exePath = Environment.ProcessPath!;
            key.SetValue(AppName, $"\"{exePath}\"");
        }
    }

    protected override void OnExit(ExitEventArgs e)
    {
        if (_trayIcon != null)
        {
            _trayIcon.Visible = false;
            _trayIcon.Dispose();
        }

        _showSignal?.Set();   // unblock the listener thread
        _showSignal?.Dispose();
        _mutex?.ReleaseMutex();
        _mutex?.Dispose();

        base.OnExit(e);
    }
}
