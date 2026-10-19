using System.Text.Json;
using System.Text.Json.Nodes;
using Filedock.Core.V1;
using FileDock.Management.Api.Auth;
using FileDock.Management.Core;
using FileDock.Management.Domain.Audit;
using Google.Protobuf;
using Google.Protobuf.WellKnownTypes;
namespace FileDock.Management.Api.Endpoints;

public static class ConnectionEndpoints
{
    private static readonly JsonSerializerOptions WebJson = new(JsonSerializerDefaults.Web);
    public sealed record OAuthStart(string ProviderFamily, JsonElement Config);
    public sealed record OAuthFinish(string FlowId, string RedirectUrl);
    public static void MapConnections(this RouteGroupBuilder api)
    {
        api.MapGet("/connections", (CoreClient core, HttpContext c) => ObservabilityEndpoints.Run(() => Page(core, c))).WithMetadata(new AccessScope("R"));
        api.MapGet("/connections/options", (CoreClient core, HttpContext c) => ObservabilityEndpoints.Run(async () => { var connectors = await core.Connectors.ListConnectorsAsync(new Filedock.Core.V1.Empty(), ObservabilityEndpoints.Headers(c, false)); return Results.Ok(ResourceEndpoints.Json(connectors)); })).WithMetadata(new AccessScope("R"));
        api.MapPost("/connections", (JsonElement spec, CoreClient core, HttpContext c, AuditWriter audit) => Save("", spec, core, c, audit)).WithMetadata(new AccessScope("F"));
        api.MapPatch("/connections/{id}", (string id, JsonElement spec, CoreClient core, HttpContext c, AuditWriter audit) => Save(id, spec, core, c, audit)).WithMetadata(new AccessScope("F"));
        api.MapDelete("/connections/{id}", (string id, CoreClient core, HttpContext c, AuditWriter audit) => ObservabilityEndpoints.Run(() => ResourceEndpoints.Delete(core, c, audit, ResourceKind.CloudConnection, id))).WithMetadata(new AccessScope("F"));
        api.MapPost("/connections/{id}/test", (string id, CoreClient core, HttpContext c, AuditWriter audit) => ObservabilityEndpoints.Run(async () => { var result = await core.Connectors.TestConnectionAsync(new ConnectionRef { ConnectionId = id }, ObservabilityEndpoints.Headers(c, true)); await ObservabilityEndpoints.Audit(c, audit, "connection.tested", id); return Results.Ok(ResourceEndpoints.Json(result)); })).WithMetadata(new AccessScope("F"));
        api.MapPost("/connections/oauth/start", (OAuthStart body, CoreClient core, HttpContext c) => ObservabilityEndpoints.Run(async () => Results.Ok(ResourceEndpoints.Json(await core.Connectors.StartOAuthAsync(new OAuthStartRequest { ProviderFamily = body.ProviderFamily, Config = Struct.Parser.ParseJson(body.Config.ValueKind == JsonValueKind.Undefined ? "{}" : body.Config.GetRawText()) }, ObservabilityEndpoints.Headers(c, true)))))).WithMetadata(new AccessScope("F"));
        api.MapPost("/connections/oauth/finish", (OAuthFinish body, CoreClient core, HttpContext c) => ObservabilityEndpoints.Run(async () => Results.Ok(ResourceEndpoints.Json(await core.Connectors.FinishOAuthAsync(new OAuthFinishRequest { FlowId = body.FlowId, RedirectUrl = body.RedirectUrl }, ObservabilityEndpoints.Headers(c, true)))))).WithMetadata(new AccessScope("F"));
    }
    private static async Task<IResult> Page(CoreClient core, HttpContext c)
    {
        var p = FileDock.Management.Api.Infrastructure.PageQuery.From(c.Request);
        var headers = ObservabilityEndpoints.Headers(c, false);
        var page = await core.Control.ListResourcesAsync(new ListResourcesRequest { Kind = ResourceKind.CloudConnection, Page = new PageRequest { PageSize = (uint)Math.Clamp(p.PageSize, 1, 500), PageToken = p.PageToken ?? "" } }, headers);
        var routes = await core.Connectors.GetRoutesAsync(new Filedock.Core.V1.Empty(), headers);
        var items = page.Resources.Select(r => {
            var view = JsonSerializer.SerializeToNode(ResourceEndpoints.View(r), WebJson)!.AsObject();
            var family = r.Spec.Fields["providerFamily"].StringValue;
            var targets = routes.Overrides.Where(x => x.Key.ConnectionId == r.Ref.Id).Concat(routes.Defaults.Where(x => x.Key.Family == family)).GroupBy(x => x.Key.Use).ToDictionary(g => g.Key.ToString(), g => g.First().Target.TargetCase == RouteTarget.TargetOneofCase.BackupEngine ? "backup-engine" : g.First().Target.ConnectorId);
            view["effectiveConnectors"] = JsonSerializer.SerializeToNode(targets); return view;
        });
        return Results.Ok(new { items, nextPageToken = string.IsNullOrEmpty(page.NextPageToken) ? null : page.NextPageToken });
    }
    private static Task<IResult> Save(string id, JsonElement input, CoreClient core, HttpContext c, AuditWriter audit) => ObservabilityEndpoints.Run(async () =>
    {
        var spec = JsonNode.Parse(input.GetRawText())!.AsObject(); var family = spec["providerFamily"]?.GetValue<string>() ?? "";
        var connectors = await core.Connectors.ListConnectorsAsync(new Filedock.Core.V1.Empty(), ObservabilityEndpoints.Headers(c, false));
        if (!connectors.Connectors.Any(x => x.Enabled && x.Families.Any(f => f.Family == family)))
        {
            return Results.Problem(statusCode: 422, detail: "connection.family_unserved");
        }

        var definition = spec["definition"]?.AsObject() ?? throw new ArgumentException("definition required");
        if (definition.ContainsKey("sessionToken") || definition.ContainsKey("roleArn"))
        {
            return Results.Problem(statusCode: 422, detail: "connection.credential_type_unsupported");
        }

        foreach (var (field, referenceField) in new[] { ("accessKey", "accessKeyRef"), ("secretKey", "secretKeyRef") })
        {
            if (definition[field] is JsonValue value) { var keyName = field == "accessKey" ? "access-key" : "secret-key"; var reference = $"connection/{Guid.NewGuid():N}/{keyName}"; await core.Secrets.PutSecretAsync(new PutSecretRequest { Ref = reference, Value = ByteString.CopyFromUtf8(value.GetValue<string>()), Consumers = { "fd-s3", "rclone", "backup-engine" } }, ObservabilityEndpoints.Headers(c, true)); definition.Remove(field); definition[referenceField] = reference; }
        }
        var row = await ResourceEndpoints.Save(core, c, ResourceKind.CloudConnection, id, spec); await ObservabilityEndpoints.Audit(c, audit, "connection.saved", row.Ref.Id); return Results.Json(ResourceEndpoints.View(row), statusCode: id.Length == 0 ? 201 : 200);
    });
}
