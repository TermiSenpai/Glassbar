using System.IO;
using System.Runtime.InteropServices;
using System.Text.Json;
using System.Windows;
using System.Windows.Interop;
using System.Windows.Media.Imaging;
using BatLauncher.Models;

namespace BatLauncher.Services;

public static class DataService
{
    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    private static extern int PrivateExtractIcons(
        string lpszFile, int nIconIndex, int cxIcon, int cyIcon,
        IntPtr[] phicon, int[] piconid, int nIcons, int flags);

    [DllImport("user32.dll")]
    private static extern bool DestroyIcon(IntPtr hIcon);
    private static readonly string AppFolder = Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
        "BatLauncher");

    private static readonly string DataFile = Path.Combine(AppFolder, "data.json");
    private static readonly string IconsFolder = Path.Combine(AppFolder, "icons");

    private static readonly JsonSerializerOptions JsonOptions = new()
    {
        WriteIndented = true
    };

    public static string GetIconsFolder()
    {
        Directory.CreateDirectory(IconsFolder);
        return IconsFolder;
    }

    public static AppData Load()
    {
        try
        {
            if (!File.Exists(DataFile))
                return new AppData();

            var json = File.ReadAllText(DataFile);
            return JsonSerializer.Deserialize<AppData>(json, JsonOptions) ?? new AppData();
        }
        catch
        {
            return new AppData();
        }
    }

    public static void Save(AppData data)
    {
        Directory.CreateDirectory(AppFolder);
        var json = JsonSerializer.Serialize(data, JsonOptions);
        File.WriteAllText(DataFile, json);
    }

    public static string? CopyIcon(string sourcePath, string entryId)
    {
        try
        {
            Directory.CreateDirectory(IconsFolder);
            var ext = Path.GetExtension(sourcePath);
            var destPath = Path.Combine(IconsFolder, $"{entryId}{ext}");
            File.Copy(sourcePath, destPath, true);
            return destPath;
        }
        catch
        {
            return null;
        }
    }

    public static string? ExtractExeIcon(string exePath, string entryId)
    {
        try
        {
            var destPath = Path.Combine(Path.GetTempPath(), $"BatLauncher_{entryId}.png");

            var hIcons = new IntPtr[1];
            var ids = new int[1];

            foreach (int size in new[] { 256, 48, 32 })
            {
                int count = PrivateExtractIcons(exePath, 0, size, size, hIcons, ids, 1, 0);
                if (count > 0 && hIcons[0] != IntPtr.Zero)
                {
                    var bitmapSource = Imaging.CreateBitmapSourceFromHIcon(
                        hIcons[0], Int32Rect.Empty, BitmapSizeOptions.FromEmptyOptions());
                    bitmapSource.Freeze();

                    var encoder = new PngBitmapEncoder();
                    encoder.Frames.Add(BitmapFrame.Create(bitmapSource));
                    using var stream = File.Create(destPath);
                    encoder.Save(stream);

                    DestroyIcon(hIcons[0]);
                    return destPath;
                }
            }

            return null;
        }
        catch
        {
            return null;
        }
    }

    public static void DeleteIcon(string? iconPath)
    {
        try
        {
            if (!string.IsNullOrEmpty(iconPath) && File.Exists(iconPath))
                File.Delete(iconPath);
        }
        catch { }
    }
}
