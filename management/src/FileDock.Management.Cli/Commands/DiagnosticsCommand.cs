namespace FileDock.Management.Cli.Commands;

public static class DiagnosticsCommand
{
    public static async Task<int> RunAsync(string socket, string output)
    {
        using var http = StateCommands.Client(socket);
        using var response = await http.PostAsync("/admin/diagnostics", null);
        if (!response.IsSuccessStatusCode) { Console.Error.WriteLine(await response.Content.ReadAsStringAsync()); return 1; }
        await using var file = new FileStream(output, FileMode.CreateNew, FileAccess.Write, FileShare.None, 4096, FileOptions.Asynchronous);
        if (!OperatingSystem.IsWindows()) { File.SetUnixFileMode(output, UnixFileMode.UserRead | UnixFileMode.UserWrite); }
        await response.Content.CopyToAsync(file); Console.WriteLine(output); return 0;
    }
}
