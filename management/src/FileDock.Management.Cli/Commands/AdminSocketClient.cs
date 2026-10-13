using System.Net.Http.Json;
using System.Net.Sockets;
namespace FileDock.Management.Cli.Commands;

internal static class AdminSocketClient
{
    public static async Task<int> PostAsync(string socketPath, string path, object? body, bool json)
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
        Console.WriteLine(json ? result : "Completed successfully.");
        return 0;
    }
}
