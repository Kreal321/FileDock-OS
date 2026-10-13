using System.Net;
using System.Net.NetworkInformation;
using System.Text.Json;
using FileDock.Management.Data;
using Microsoft.EntityFrameworkCore;

namespace FileDock.Management.Api.Network;

public sealed class NetworkService(AppDbContext db, ListenerConfiguration listener)
{
    private static readonly SemaphoreSlim Gate = new(1, 1);
    public static readonly string[] DefaultAddresses = ["::"];
    public async Task<string[]> GetAsync()
    {
        var settings = await db.Settings.SingleAsync(s => s.Id == 1);
        return settings.ListenAddresses is null ? DefaultAddresses : JsonSerializer.Deserialize<string[]>(settings.ListenAddresses)!;
    }
    public async Task<string[]> GetBindableAsync()
    {
        var addresses = await GetAsync();
        var local = NetworkInterface.GetAllNetworkInterfaces().SelectMany(i => i.GetIPProperties().UnicastAddresses)
            .Select(a => a.Address.ToString()).ToHashSet(StringComparer.Ordinal);
        local.Add("127.0.0.1"); local.Add("::1");
        // Keep the console socket available if a configured interface disappeared.
        // Never widen the public listener automatically.
        return addresses.Where(a => a is "::" or "0.0.0.0" || local.Contains(a)).ToArray();
    }
    public async Task<string[]> SaveAsync(string[]? addresses, IPAddress? currentAddress, bool confirmDisconnect)
    {
        if (addresses is null || addresses.Length == 0)
        {
            throw new ArgumentException("Choose at least one listener address.");
        }

        var local = NetworkInterface.GetAllNetworkInterfaces()
            .SelectMany(i => i.GetIPProperties().UnicastAddresses).Select(a => a.Address).ToHashSet();
        local.Add(IPAddress.Loopback); local.Add(IPAddress.IPv6Loopback);
        var normalized = new List<string>();
        foreach (var value in addresses)
        {
            if (!IPAddress.TryParse(value, out var ip) || (!local.Contains(ip) && !ip.Equals(IPAddress.Any) && !ip.Equals(IPAddress.IPv6Any)))
            {
                throw new ArgumentException($"'{value}' is not an IP address on this appliance.");
            }

            if (ip.IsIPv4MappedToIPv6)
            {
                ip = ip.MapToIPv4();
            }

            if (!normalized.Contains(ip.ToString(), StringComparer.Ordinal))
            {
                normalized.Add(ip.ToString());
            }
        }
        if (normalized.Count > 1 && normalized.Any(a => a is "0.0.0.0" or "::"))
        {
            throw new ArgumentException("Use one wildcard address or a list of specific addresses.");
        }

        var current = currentAddress ?? IPAddress.Loopback;
        if (current.IsIPv4MappedToIPv6)
        {
            current = current.MapToIPv4();
        }

        if (!confirmDisconnect && !normalized.Contains(current.ToString(), StringComparer.Ordinal)
            && !normalized.Contains("::", StringComparer.Ordinal)
            && !(current.AddressFamily == System.Net.Sockets.AddressFamily.InterNetwork && normalized.Contains("0.0.0.0", StringComparer.Ordinal)))
        {
            throw new DisconnectConfirmationException();
        }

        await Gate.WaitAsync();
        try
        {
            var settings = await db.Settings.SingleAsync(s => s.Id == 1);
            settings.ListenAddresses = JsonSerializer.Serialize(normalized);
            await db.SaveChangesAsync();
        }
        finally { Gate.Release(); }
        return normalized.ToArray();
    }
    public void Apply(string[] addresses) => listener.Apply(addresses);
}
public sealed class DisconnectConfirmationException : Exception
{
    public DisconnectConfirmationException() : base("This change removes the address used by this connection. Set confirmDisconnect to true to continue. Use filedockctl network reset from the console if needed.") { }
}
