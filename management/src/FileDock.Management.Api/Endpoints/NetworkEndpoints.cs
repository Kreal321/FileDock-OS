using FileDock.Management.Api.Auth;
using FileDock.Management.Api.Network;
using FileDock.Management.Domain.Audit;

namespace FileDock.Management.Api.Endpoints;

public static class NetworkEndpoints
{
    public static void MapNetwork(this RouteGroupBuilder api)
    {
        api.MapGet("/system/network", async (NetworkService network, ListenerConfiguration listener) =>
            Results.Ok(new
            {
                listenAddresses = await network.GetAsync(),
                port = listener.Port,
                availableAddresses = System.Net.NetworkInformation.NetworkInterface.GetAllNetworkInterfaces()
                .SelectMany(i => i.GetIPProperties().UnicastAddresses.Select(a => new { interfaceName = i.Name, address = a.Address.ToString() }))
            })).WithMetadata(new AccessScope("R"));
        api.MapPut("/system/network", async (NetworkRequest body, HttpContext context, NetworkService network, AuditWriter audit) =>
        {
            try
            {
                var addresses = await network.SaveAsync(body.ListenAddresses, context.Connection.LocalIpAddress, body.ConfirmDisconnect);
                await AdminEndpoints.WriteAudit(context, audit, "network.update", Guid.Empty);
                context.Response.OnCompleted(() => { network.Apply(addresses); return Task.CompletedTask; });
                return Results.Ok(new { listenAddresses = addresses });
            }
            catch (DisconnectConfirmationException ex) { return AdminEndpoints.Problem(409, ex.Message); }
            catch (ArgumentException ex) { return AdminEndpoints.Problem(400, ex.Message); }
        }).WithMetadata(new AccessScope("S"));
    }
}
public sealed record NetworkRequest(string[]? ListenAddresses, bool ConfirmDisconnect = false);
