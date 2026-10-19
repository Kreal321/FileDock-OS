using System.Net.Http.Json;
using System.Net.Sockets;
namespace FileDock.Management.Cli.Commands;

internal static class AdminSocketClient
{
    private static readonly System.Text.Json.JsonSerializerOptions PrettyJson = new() { WriteIndented = true };
    public static async Task<int> PostAsync(string socketPath, string path, object? body, bool json, bool showResult = false)
    {
        using var handler = new SocketsHttpHandler
        {
            ConnectCallback = async (_, cancellationToken) =>
            {
                var socket = new Socket(AddressFamily.Unix, SocketType.Stream, ProtocolType.Unspecified);
                try { await socket.ConnectAsync(new UnixDomainSocketEndPoint(socketPath), cancellationToken); return new NetworkStream(socket, ownsSocket: true); }
                catch { socket.Dispose(); throw; }
            },
        };
        using var http = new HttpClient(handler) { BaseAddress = new Uri("http://localhost") };
        using var response = body is null ? await http.PostAsync(path, null) : await http.PostAsJsonAsync(path, body);
        var result = await response.Content.ReadAsStringAsync();
        if (!response.IsSuccessStatusCode) { Console.Error.WriteLine(result); return 1; }
        if (json) { Console.WriteLine(result); }
        else if (showResult)
        {
            using var document = System.Text.Json.JsonDocument.Parse(result);
            Console.WriteLine(System.Text.Json.JsonSerializer.Serialize(document.RootElement, PrettyJson));
        }
        else { Console.WriteLine("Completed successfully."); }
        return 0;
    }
}
