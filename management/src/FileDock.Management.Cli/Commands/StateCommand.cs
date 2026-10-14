using System.Net.Http.Json;
using System.Net.Sockets;
using System.Text.Json;
namespace FileDock.Management.Cli.Commands;

internal static class StateCommands
{
    private static HttpClient Client(string path)
    {
        var handler = new SocketsHttpHandler
        {
            ConnectCallback = async (_, cancellationToken) =>
        {
            var socket = new Socket(AddressFamily.Unix, SocketType.Stream, ProtocolType.Unspecified);
            try { await socket.ConnectAsync(new UnixDomainSocketEndPoint(path), cancellationToken); return new NetworkStream(socket, true); } catch { socket.Dispose(); throw; }
        }
        };
        return new HttpClient(handler) { BaseAddress = new Uri("http://localhost"), Timeout = TimeSpan.FromMinutes(2) };
    }
    private static string Hidden(string prompt)
    {
        Console.Error.Write(prompt); var value = new System.Text.StringBuilder();
        if (Console.IsInputRedirected) { throw new InvalidOperationException("Enter the recovery key at an interactive console."); }
        while (true) { var key = Console.ReadKey(true); if (key.Key == ConsoleKey.Enter) { break; } if (key.Key == ConsoleKey.Backspace) { if (value.Length > 0) { value.Length--; } } else if (!char.IsControl(key.KeyChar)) { value.Append(key.KeyChar); } }
        Console.Error.WriteLine(); return value.ToString();
    }
    public static async Task<int> Export(string socket, string output)
    {
        using var http = Client(socket); using var response = await http.PostAsync("/admin/backup-file", null);
        if (!response.IsSuccessStatusCode) { Console.Error.WriteLine(await response.Content.ReadAsStringAsync()); return 1; }
        await using var file = new FileStream(output, FileMode.CreateNew, FileAccess.Write, FileShare.None, 65536, FileOptions.Asynchronous);
        if (!OperatingSystem.IsWindows()) { File.SetUnixFileMode(output, UnixFileMode.UserRead | UnixFileMode.UserWrite); }
        await response.Content.CopyToAsync(file); await file.FlushAsync(); Console.WriteLine(output); return 0;
    }
    public static async Task<int> Restore(string socket, bool json)
    {
        using var http = Client(socket); using var detect = await http.PostAsync("/admin/detect-state", null); if (!detect.IsSuccessStatusCode) { Console.Error.WriteLine(await detect.Content.ReadAsStringAsync()); return 1; }
        using var doc = JsonDocument.Parse(await detect.Content.ReadAsStringAsync()); if (!doc.RootElement.TryGetProperty("appliances", out var found) || found.GetArrayLength() == 0) { Console.Error.WriteLine("No local recovery copy found."); return 1; }
        var items = found.EnumerateArray().ToArray(); for (var i = 0; i < items.Length; i++) { Console.Error.WriteLine($"{i + 1}. {items[i].GetProperty("applianceName").GetString()}"); }
        var selected = 0; if (items.Length > 1) { Console.Error.Write("Appliance number: "); if (!int.TryParse(Console.ReadLine(), out var number) || number < 1 || number > items.Length) { return 1; } selected = number - 1; }
        var key = Hidden("Recovery key: "); using var response = await http.PostAsJsonAsync("/admin/restore-local", new { localSourceToken = items[selected].GetProperty("localSourceToken").GetString(), recoveryKey = key }); key = "";
        var result = await response.Content.ReadAsStringAsync(); if (!response.IsSuccessStatusCode) { Console.Error.WriteLine(result); return 1; }
        using var op = JsonDocument.Parse(result); var id = op.RootElement.GetProperty("id").GetString();
        var deadline = DateTimeOffset.UtcNow.AddMinutes(30);
        while (DateTimeOffset.UtcNow < deadline)
        {
            using var check = await http.PostAsJsonAsync("/admin/restore-operation", new { id });
            if (!check.IsSuccessStatusCode) { Console.Error.WriteLine("Restore status could not be read. Check the Core service and management UI."); return 1; }
            var progress = await check.Content.ReadAsStringAsync(); using var document = JsonDocument.Parse(progress); var state = document.RootElement.GetProperty("state").GetString();
            if (state == "Failed") { Console.Error.WriteLine("Restore failed. Inspect the operation in the management UI."); return 1; }
            var imported = document.RootElement.GetProperty("steps").EnumerateArray().Any(s => s.GetProperty("name").GetString() == "import-management" && s.GetProperty("state").GetString() == "done");
            if (state == "Succeeded" || imported) { Console.WriteLine(json ? progress : $"Configuration restored: {id}. Sign in and review the restore report for pending storage or extension work."); return 0; }
            await Task.Delay(2000);
        }
        Console.Error.WriteLine("Restore did not complete within 30 minutes. Review its operation before retrying."); return 1;
    }
    public static async Task<int> Regenerate(string socket, string output)
    {
        if (File.Exists(output) || Directory.Exists(output)) { throw new IOException("Choose a new recovery-key file path."); }
        var probe = output + "." + Guid.NewGuid().ToString("N") + ".tmp";
        using (var check = new FileStream(probe, FileMode.CreateNew, FileAccess.Write, FileShare.None)) { check.Flush(true); }
        File.Delete(probe);
        using var http = Client(socket); using var response = await http.PostAsync("/admin/recovery-key/regenerate", null); if (!response.IsSuccessStatusCode) { Console.Error.WriteLine(await response.Content.ReadAsStringAsync()); return 1; }
        using var doc = JsonDocument.Parse(await response.Content.ReadAsStringAsync()); using (var file = new FileStream(output, FileMode.CreateNew, FileAccess.Write, FileShare.None)) { if (!OperatingSystem.IsWindows()) { File.SetUnixFileMode(output, UnixFileMode.UserRead | UnixFileMode.UserWrite); } var bytes = System.Text.Encoding.UTF8.GetBytes(doc.RootElement.GetProperty("recoveryKey").GetString() + "\n"); file.Write(bytes); file.Flush(true); }
        using var confirm = await http.PostAsJsonAsync("/admin/recovery-key/confirm", new { keyId = doc.RootElement.GetProperty("keyId").GetString() }); if (!confirm.IsSuccessStatusCode) { Console.Error.WriteLine("Key saved, but confirmation failed. Retry confirmation before relying on new snapshots."); return 1; }
        Console.WriteLine($"Recovery key saved to {output}. Keep it off the appliance."); return 0;
    }
}
