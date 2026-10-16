using System.Text.Json;
using Filedock.Core.V1;
using FileDock.Management.Api.Auth;
using FileDock.Management.Core;
using FileDock.Management.Domain.Audit;
using Google.Protobuf;
namespace FileDock.Management.Api.Endpoints;

public static class ShareEndpoints
{
    public sealed record UserInput(string Username, string? Password, bool Enabled = true);
    public sealed record ExportInput(JsonElement[] Rules);
    private static async Task<IResult> User(string id, UserInput body, CoreClient core, HttpContext c, AuditWriter audit)
    {
        return await ObservabilityEndpoints.Run(async () =>
        {
            string reference;
            if (id.Length > 0) { var old = await core.Control.GetResourceAsync(new ResourceRef { Kind = ResourceKind.ShareUser, Id = id }, ObservabilityEndpoints.Headers(c, false)); var spec = ResourceEndpoints.Json(old.Spec); if (spec.GetProperty("username").GetString() != body.Username) { return Results.Problem(statusCode: 422, detail: "share_user.username_immutable"); } reference = spec.GetProperty("passwordSecret").GetString()!; }
            else { if (string.IsNullOrEmpty(body.Password)) { return Results.Problem(statusCode: 422, detail: "share_user.password_required"); } reference = $"share-user/{Guid.NewGuid():N}/password"; }
            if (body.Password is not null) { reference = $"share-user/{Guid.NewGuid():N}/password"; if (body.Password.Length == 0 || body.Password.Contains('\n') || body.Password.Contains('\r')) { return Results.Problem(statusCode: 422, detail: "share_user.password_invalid"); } await core.Secrets.PutSecretAsync(new PutSecretRequest { Ref = reference, Value = ByteString.CopyFromUtf8(body.Password) }, ObservabilityEndpoints.Headers(c, true)); }
            var row = await ResourceEndpoints.Save(core, c, ResourceKind.ShareUser, id, new { body.Username, passwordSecret = reference, body.Enabled }); await ObservabilityEndpoints.Audit(c, audit, "share-user.saved", row.Ref.Id); return Results.Json(ResourceEndpoints.View(row), statusCode: id.Length == 0 ? 201 : 200);
        });
    }
    public static void MapShares(this RouteGroupBuilder api)
    {
        api.MapGet("/share-users", (CoreClient core, HttpContext c) => ObservabilityEndpoints.Run(() => ResourceEndpoints.Page(core, ResourceKind.ShareUser, c))).WithMetadata(new AccessScope("R"));
        api.MapPost("/share-users", (UserInput body, CoreClient core, HttpContext c, AuditWriter audit) => User("", body, core, c, audit)).WithMetadata(new AccessScope("F"));
        api.MapPatch("/share-users/{id}", (string id, UserInput body, CoreClient core, HttpContext c, AuditWriter audit) => User(id, body, core, c, audit)).WithMetadata(new AccessScope("F"));
        api.MapDelete("/share-users/{id}", (string id, CoreClient core, HttpContext c, AuditWriter audit) => ObservabilityEndpoints.Run(() => ResourceEndpoints.Delete(core, c, audit, ResourceKind.ShareUser, id))).WithMetadata(new AccessScope("F"));
        api.MapGet("/folders/{id}/exports", (string id, CoreClient core, HttpContext c) => ObservabilityEndpoints.Run(async () => Results.Ok(new { items = (await ResourceEndpoints.All(core, ResourceKind.Export, c)).Where(r => ResourceEndpoints.Json(r.Spec).GetProperty("folderId").GetString() == id).Select(ResourceEndpoints.View) }))).WithMetadata(new AccessScope("R"));
        api.MapPut("/folders/{id}/exports/{protocol}", (string id, string protocol, ExportInput body, CoreClient core, HttpContext c, AuditWriter audit) => ObservabilityEndpoints.Run(async () =>
        {
            var p = protocol switch { "smb" => "Smb", "nfs" => "Nfs", "webdav" => "WebDav", _ => "" }; if (p.Length == 0)
            {
                return Results.NotFound();
            }

            var existing = (await ResourceEndpoints.All(core, ResourceKind.Export, c)).SingleOrDefault(r => ResourceEndpoints.Json(r.Spec).GetProperty("folderId").GetString() == id && ResourceEndpoints.Json(r.Spec).GetProperty("protocol").GetString() == p);
            var row = await ResourceEndpoints.Save(core, c, ResourceKind.Export, existing?.Ref.Id ?? "", new { folderId = id, protocol = p, body.Rules }); await ObservabilityEndpoints.Audit(c, audit, "export.rules.changed", row.Ref.Id); return Results.Ok(ResourceEndpoints.View(row));
        })).WithMetadata(new AccessScope("F"));
    }
}
