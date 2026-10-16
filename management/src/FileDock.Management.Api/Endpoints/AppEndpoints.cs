using System.Text.Json;
using Filedock.Core.V1;
using FileDock.Management.Api.Auth;
using FileDock.Management.Core;
using FileDock.Management.Domain.Audit;
using Google.Protobuf.WellKnownTypes;
namespace FileDock.Management.Api.Endpoints;

public static class AppEndpoints
{
    public static void MapApps(this RouteGroupBuilder api)
    {
        api.MapGet("/apps", (CoreClient core, HttpContext c) => ObservabilityEndpoints.Run(() => ResourceEndpoints.Page(core, ResourceKind.ContainerApp, c))).WithMetadata(new AccessScope("R"));
        api.MapPost("/apps", (JsonElement spec, CoreClient core, HttpContext c, AuditWriter audit) => Save("", spec, core, c, audit)).WithMetadata(new AccessScope("F"));
        api.MapPatch("/apps/{id}", (string id, JsonElement spec, CoreClient core, HttpContext c, AuditWriter audit) => Save(id, spec, core, c, audit)).WithMetadata(new AccessScope("F"));
        api.MapDelete("/apps/{id}", (string id, CoreClient core, HttpContext c, AuditWriter audit) => ObservabilityEndpoints.Run(() => ResourceEndpoints.Delete(core, c, audit, ResourceKind.ContainerApp, id))).WithMetadata(new AccessScope("F"));
        api.MapPost("/apps/{id}/{action}", (string id, string action, CoreClient core, HttpContext c, AuditWriter audit) => ObservabilityEndpoints.Run(async () => { if (action is not ("start" or "stop" or "update")) { return Results.NotFound(); } var op = await core.Control.StartOperationAsync(new StartOperationRequest { Kind = "AppAction", Params = Struct.Parser.ParseJson(JsonSerializer.Serialize(new { id, action })) }, ObservabilityEndpoints.Headers(c, true)); await ObservabilityEndpoints.Audit(c, audit, $"app.{action}", id); return Results.Accepted($"/api/v1/operations/{op.Id}", CoreClient.ToOperation(op)); })).WithMetadata(new AccessScope("F"));
    }
    private static Task<IResult> Save(string id, JsonElement spec, CoreClient core, HttpContext c, AuditWriter audit) => ObservabilityEndpoints.Run(async () => { var row = await ResourceEndpoints.Save(core, c, ResourceKind.ContainerApp, id, spec); await ObservabilityEndpoints.Audit(c, audit, "app.saved", row.Ref.Id); return Results.Json(ResourceEndpoints.View(row), statusCode: id.Length == 0 ? 201 : 200); });
}
