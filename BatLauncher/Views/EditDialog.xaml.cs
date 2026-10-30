using System.IO;
using System.Windows;
using System.Windows.Input;
using System.Windows.Media.Imaging;
using BatLauncher.Models;
using BatLauncher.Services;
using Microsoft.Win32;

namespace BatLauncher.Views;

public partial class EditDialog : Window
{
    private readonly BatEntry _entry;
    private string? _selectedIconPath;
    private bool _iconRemoved;
    private bool _isAutoExtractedIcon;

    public BatEntry? Result { get; private set; }

    public EditDialog(BatEntry entry, bool isNew)
    {
        InitializeComponent();
        _entry = entry;

        DialogTitle.Text = isNew ? "Add Entry" : "Edit Entry";
        NameBox.Text = entry.Name;
        FilePathBox.Text = entry.FilePath;
        _selectedIconPath = entry.IconPath;

        if (!string.IsNullOrEmpty(entry.IconPath) && File.Exists(entry.IconPath))
        {
            LoadIconPreview(entry.IconPath);
            RemoveIconBtn.Visibility = Visibility.Visible;
        }
        else if (!string.IsNullOrEmpty(entry.FilePath) &&
                 Path.GetExtension(entry.FilePath).Equals(".exe", StringComparison.OrdinalIgnoreCase) &&
                 File.Exists(entry.FilePath))
        {
            TryExtractExeIcon(entry.FilePath);
        }

        NameBox.Focus();
    }

    private void TitleBar_MouseLeftButtonDown(object sender, MouseButtonEventArgs e)
    {
        if (e.ClickCount == 1)
            DragMove();
    }

    private void BrowseFile_Click(object sender, RoutedEventArgs e)
    {
        var dlg = new OpenFileDialog
        {
            Title = "Select a file to launch",
            Filter = "Supported files (*.bat;*.cmd;*.exe)|*.bat;*.cmd;*.exe|Batch files (*.bat;*.cmd)|*.bat;*.cmd|Executables (*.exe)|*.exe|All files (*.*)|*.*",
            CheckFileExists = true
        };

        if (dlg.ShowDialog(this) == true)
        {
            FilePathBox.Text = dlg.FileName;
            if (string.IsNullOrWhiteSpace(NameBox.Text))
                NameBox.Text = Path.GetFileNameWithoutExtension(dlg.FileName);

            // Auto-extract icon for EXE files when no custom icon is set
            var ext = Path.GetExtension(dlg.FileName).ToLowerInvariant();
            if (ext == ".exe" && (_selectedIconPath == null || _isAutoExtractedIcon))
                TryExtractExeIcon(dlg.FileName);
        }
    }

    private void BrowseIcon_Click(object sender, RoutedEventArgs e)
    {
        var dlg = new OpenFileDialog
        {
            Title = "Select an icon image",
            Filter = "Images (*.png;*.jpg;*.jpeg;*.ico;*.bmp)|*.png;*.jpg;*.jpeg;*.ico;*.bmp|All files (*.*)|*.*",
            CheckFileExists = true
        };

        if (dlg.ShowDialog(this) == true)
        {
            _selectedIconPath = dlg.FileName;
            _iconRemoved = false;
            _isAutoExtractedIcon = false;
            LoadIconPreview(dlg.FileName);
            RemoveIconBtn.Visibility = Visibility.Visible;
        }
    }

    private void RemoveIcon_Click(object sender, RoutedEventArgs e)
    {
        _selectedIconPath = null;
        _iconRemoved = true;
        _isAutoExtractedIcon = false;
        IconPreview.Source = null;
        RemoveIconBtn.Visibility = Visibility.Collapsed;
    }

    private void TryExtractExeIcon(string exePath)
    {
        var extractedPath = DataService.ExtractExeIcon(exePath, _entry.Id);
        if (extractedPath != null)
        {
            _selectedIconPath = extractedPath;
            _isAutoExtractedIcon = true;
            _iconRemoved = false;
            LoadIconPreview(extractedPath);
            RemoveIconBtn.Visibility = Visibility.Visible;
        }
    }

    private void LoadIconPreview(string path)
    {
        try
        {
            var bitmap = new BitmapImage();
            bitmap.BeginInit();
            bitmap.UriSource = new Uri(path, UriKind.Absolute);
            bitmap.CacheOption = BitmapCacheOption.OnLoad;
            bitmap.DecodePixelWidth = 96;
            bitmap.EndInit();
            bitmap.Freeze();
            IconPreview.Source = bitmap;
        }
        catch
        {
            IconPreview.Source = null;
        }
    }

    private void Save_Click(object sender, RoutedEventArgs e)
    {
        var name = NameBox.Text.Trim();
        var filePath = FilePathBox.Text.Trim();

        if (string.IsNullOrEmpty(name))
        {
            MessageBox.Show("Please enter a display name.", "Validation", MessageBoxButton.OK, MessageBoxImage.Warning);
            NameBox.Focus();
            return;
        }

        if (string.IsNullOrEmpty(filePath) || !File.Exists(filePath))
        {
            MessageBox.Show("Please select a valid file.", "Validation", MessageBoxButton.OK, MessageBoxImage.Warning);
            return;
        }

        _entry.Name = name;
        _entry.FilePath = filePath;

        // Handle icon
        if (_iconRemoved)
        {
            DataService.DeleteIcon(_entry.IconPath);
            _entry.IconPath = null;
        }
        else if (_selectedIconPath != null && _selectedIconPath != _entry.IconPath)
        {
            DataService.DeleteIcon(_entry.IconPath);
            _entry.IconPath = DataService.CopyIcon(_selectedIconPath, _entry.Id);
        }

        Result = _entry;
        DialogResult = true;
        Close();
    }

    private void Cancel_Click(object sender, RoutedEventArgs e)
    {
        DialogResult = false;
        Close();
    }
}
