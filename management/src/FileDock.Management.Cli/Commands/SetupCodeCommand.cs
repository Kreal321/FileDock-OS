using System.Net.Sockets;
using System.Text.Json;

namespace FileDock.Management.Cli.Commands;

public static class SetupCodeCommand
{
    public static async Task<int> RunAsync(string socketPath, bool json)
    {
        using var handler = new SocketsHttpHandler
        {
            ConnectCallback = async (_, cancellationToken) =>
            {
                var unix = new Socket(AddressFamily.Unix, SocketType.Stream, ProtocolType.Unspecified);
                await unix.ConnectAsync(new UnixDomainSocketEndPoint(socketPath), cancellationToken);
                return new NetworkStream(unix, ownsSocket: true);
            },
        };
        using var http = new HttpClient(handler) { BaseAddress = new Uri("http://localhost") };
        using var response = await http.PostAsync("/admin/setup-code", null);
        var body = await response.Content.ReadAsStringAsync();
        if (!response.IsSuccessStatusCode)
        {
            Console.Error.WriteLine(body);
            return 1;
        }

        using var doc = JsonDocument.Parse(body);
        var code = doc.RootElement.GetProperty("code").GetString() ?? "";
        if (json)
        {
            Console.WriteLine(JsonSerializer.Serialize(new { code }));
        }
        else
        {
            Console.WriteLine($"One-time setup code: {code}");
        }

        return 0;
    }
}
