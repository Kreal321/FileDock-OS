using Filedock.Core.V1;
using FileDock.Management.Api.Auth;
using FileDock.Management.Core;
using FileDock.Management.Domain.Audit;
using Google.Protobuf;
using Grpc.Core;
namespace FileDock.Management.Api.Endpoints;

public static class ConnectorEndpoints
{
    public sealed record EnabledInput(bool Enabled);
    public sealed record TargetInput(string? ConnectorId, bool BackupEngine = false);
    public sealed record ChangeInput(string Family, string Use, string? ConnectionId, TargetInput? Target);
    public sealed record RoutesInput(ChangeInput[] Changes, ulong ExpectedGeneration, bool AcknowledgedBasicClaimSafety = false, bool AcknowledgedThirdPartyOwnedStorage = false);
    public static object Routes(RouteTable table) => new { table.Generation, defaults = table.Defaults.Select(Route), overrides = table.Overrides.Select(Route) };
    private static object Route(Filedock.Core.V1.Route r) => new { r.Key.Family, use = r.Key.Use.ToString(), connectionId = r.Key.ConnectionId, target = r.Target.TargetCase == RouteTarget.TargetOneofCase.BackupEngine ? new TargetInput(null, true) : new TargetInput(r.Target.ConnectorId) };
    public static RouteChangeRequest Request(RoutesInput body)
    {
        var request = new RouteChangeRequest { ExpectedGeneration = body.ExpectedGeneration, AcknowledgedBasicClaimSafety = body.AcknowledgedBasicClaimSafety, AcknowledgedThirdPartyOwnedStorage = body.AcknowledgedThirdPartyOwnedStorage };
        foreach (var c in body.Changes ?? [])
        {
            if (!Enum.TryParse<RouteUse>(c.Use, true, out var usage) || usage == RouteUse.Unspecified || !Enum.IsDefined(usage)) { throw new ArgumentException("route.use_not_allowed"); }
            var change = new RouteChangeRequest.Types.Change { Key = new RouteKey { Family = c.Family, Use = usage, ConnectionId = c.ConnectionId ?? "" } };
            if (c.Target is not null)
            {
                if (c.Target.BackupEngine == !string.IsNullOrEmpty(c.Target.ConnectorId)) { throw new ArgumentException("route.target_invalid"); }
                change.Target = c.Target.BackupEngine ? new RouteTarget { BackupEngine = true } : new RouteTarget { ConnectorId = c.Target.ConnectorId };
            }
            request.Changes.Add(change);
        }
        return request;
    }
    public static void MapConnectors(this RouteGroupBuilder api)
    {
        api.MapGet("/connectors", (CoreClient core, HttpContext c) => ObservabilityEndpoints.Run(async () => Results.Ok(ResourceEndpoints.Json(await core.Connectors.ListConnectorsAsync(new Filedock.Core.V1.Empty(), ObservabilityEndpoints.Headers(c, false)))))).WithMetadata(new AccessScope("R"));
        api.MapPatch("/connectors/{id}", (string id, EnabledInput body, CoreClient core, HttpContext c, AuditWriter audit) => ObservabilityEndpoints.Run(async () => {
            try
            {
                var result = await core.Connectors.SetConnectorEnabledAsync(new SetConnectorEnabledRequest { SubsystemId = id, Enabled = body.Enabled }, ObservabilityEndpoints.Headers(c, true));
                await ObservabilityEndpoints.Audit(c, audit, "connector.enabled_changed", id, new { body.Enabled });
                return Results.Ok(ResourceEndpoints.Json(result));
            }
            catch (RpcException error) when (error.StatusCode == StatusCode.AlreadyExists && error.Status.Detail == "connector.in_use")
            {
                var routes = await core.Connectors.GetRoutesAsync(new Filedock.Core.V1.Empty(), ObservabilityEndpoints.Headers(c, false));
                return Results.Problem(statusCode: 409, detail: "connector.in_use", extensions: new Dictionary<string, object?> { ["routes"] = Routes(routes) });
            }
        })).WithMetadata(new AccessScope("F"));
        api.MapGet("/connector-routes", (CoreClient core, HttpContext c) => ObservabilityEndpoints.Run(async () => Results.Ok(Routes(await core.Connectors.GetRoutesAsync(new Filedock.Core.V1.Empty(), ObservabilityEndpoints.Headers(c, false)))))).WithMetadata(new AccessScope("R"));
        api.MapPost("/connector-routes/preview", (RoutesInput body, CoreClient core, HttpContext c) => ObservabilityEndpoints.Run(async () => Results.Ok(ResourceEndpoints.Json(await core.Connectors.PreviewRouteChangeAsync(Request(body), ObservabilityEndpoints.Headers(c, false)))))).WithMetadata(new AccessScope("F"));
        api.MapPut("/connector-routes", (RoutesInput body, CoreClient core, HttpContext c, AuditWriter audit) => ObservabilityEndpoints.Run(async () => {
            var id = Guid.CreateVersion7(); var headers = ObservabilityEndpoints.Headers(c, true); headers.Add("x-fd-audit-id", id.ToString());
            var table = await core.Connectors.ApplyRouteChangeAsync(Request(body), headers);
            await audit.WriteAsync(c.Items["fd.token"] is not null ? "ApiToken" : "Administrator", c.User.FindFirst(System.Security.Claims.ClaimTypes.NameIdentifier)?.Value ?? "management", c.User.Identity?.Name ?? "management", "connector.routes_changed", "routes", "Succeeded", new { body.Changes, body.AcknowledgedBasicClaimSafety, body.AcknowledgedThirdPartyOwnedStorage, table.Generation }, auditEntryId: id);
            return Results.Ok(Routes(table));
        })).WithMetadata(new AccessScope("F"));
    }
}
