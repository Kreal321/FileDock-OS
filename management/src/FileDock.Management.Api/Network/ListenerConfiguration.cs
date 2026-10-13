using System.Net;
using Microsoft.Extensions.Configuration;

namespace FileDock.Management.Api.Network;

// A dedicated configuration source keeps all public endpoints HTTP-only and reloadable.
public sealed class ListenerConfiguration(int port) : ConfigurationProvider, IConfigurationSource
{
    public int Port { get; } = port;
    public IConfigurationProvider Build(IConfigurationBuilder builder) => this;
    public void Apply(IReadOnlyList<string> addresses)
    {
        Data = addresses.Select((address, index) => new KeyValuePair<string, string?>(
            $"Kestrel:Endpoints:Management{index}:Url",
            $"http://{(IPAddress.Parse(address).AddressFamily == System.Net.Sockets.AddressFamily.InterNetworkV6 ? $"[{address}]" : address)}:{Port}"))
            .ToDictionary(p => p.Key, p => p.Value, StringComparer.OrdinalIgnoreCase);
        OnReload();
    }
}
