using System.Security.Cryptography;
using System.Text.Json;
using Filedock.Core.V1;
using FileDock.Management.Api.Auth;
using FileDock.Management.Core;
using FileDock.Management.Domain;
using FileDock.Management.Domain.Audit;
using Google.Protobuf;
using Grpc.Core;
using Microsoft.AspNetCore.Http.Features;
namespace FileDock.Management.Api.Endpoints;

public static class ExtensionEndpoints
{
    private const long Limit = 64L * 1024 * 1024;
    private static readonly JsonSerializerOptions Json = new(JsonSerializerDefaults.Web);
    private static string Inbox(IConfiguration config) => config["Extensions:Inbox"] ?? "/var/lib/filedock/extensions/inbox";
    private static string PathFor(IConfiguration config, string id)
    {
        if (string.IsNullOrEmpty(id) || id.Length != 64 || !id.All(char.IsAsciiHexDigit)) { throw new ArgumentException("Invalid staged package identifier."); }
        return Path.Combine(Inbox(config), id + ".fdext");
    }
    private static Metadata Headers(HttpContext context, bool write) => CoreClient.Metadata(context.Items["fd.admin"] is Administrator admin ? $"admin:{admin.Username}" : context.Items["fd.token"] is ApiToken token ? $"token:{token.Id}" : "management", Guid.NewGuid().ToString("N"), write);
    private static Task Audit(AuditWriter audit, HttpContext context, string action, string target)
    {
        var admin = context.Items["fd.admin"] as Administrator; var token = context.Items["fd.token"] as ApiToken;
        return audit.WriteAsync(admin is not null ? "Administrator" : "ApiToken", admin?.Id.ToString() ?? token?.Id.ToString() ?? "management", admin?.Username ?? token?.Name ?? "management", action, target, "Accepted");
    }
    private static async Task<IResult> Run(Func<Task<IResult>> action)
    {
        try { return await action(); }
        catch (RpcException ex) { return Results.Problem(statusCode: ex.StatusCode == StatusCode.Unavailable ? 503 : ex.StatusCode == StatusCode.NotFound ? 404 : 422, detail: ex.Status.Detail); }
        catch (Exception ex) when (ex is ArgumentException or InvalidDataException or JsonException) { return Results.Problem(statusCode: 400, detail: ex.Message); }
    }
    private static async Task<string> Stage(HttpContext context, IConfiguration config)
    {
        var size = context.Features.Get<IHttpMaxRequestBodySizeFeature>(); if (size is { IsReadOnly: false }) { size.MaxRequestBodySize = Limit + 1024 * 1024; }
        if (!context.Request.HasFormContentType) { throw new ArgumentException("Upload a multipart .fdext package."); }
        var form = await context.Request.ReadFormAsync(context.RequestAborted); var file = form.Files.GetFile("file") ?? throw new ArgumentException("Select a .fdext package.");
        if (file.Length is <= 0 or > Limit) { throw new ArgumentException("Package exceeds 64 MiB."); }
        Directory.CreateDirectory(Inbox(config));
        // Garbage-collect abandoned inspections; lifecycle operations use the encrypted Core cache.
        foreach (var old in Directory.EnumerateFiles(Inbox(config), "*.fdext").Where(p => File.GetLastWriteTimeUtc(p) < DateTime.UtcNow.AddHours(-1))) { File.Delete(old); }
        var temporary = Path.Combine(Inbox(config), Guid.NewGuid().ToString("N") + ".tmp");
        try
        {
            await using (var target = new FileStream(temporary, FileMode.CreateNew, FileAccess.Write, FileShare.None))
            {
                if (!OperatingSystem.IsWindows()) { File.SetUnixFileMode(temporary, UnixFileMode.UserRead | UnixFileMode.UserWrite | UnixFileMode.GroupRead); }
                await file.CopyToAsync(target, context.RequestAborted); await target.FlushAsync(context.RequestAborted);
            }
            var id = Convert.ToHexStringLower(SHA256.HashData(await File.ReadAllBytesAsync(temporary, context.RequestAborted)));
            var path = PathFor(config, id);
            if (!File.Exists(path)) { File.Move(temporary, path); }
            return id;
        }
        finally { if (File.Exists(temporary)) { File.Delete(temporary); } }
    }
    private static object Inspection(PackageInspection value, string id) => new { stagedPackageId = id, value.Valid, Issues = value.Issues.Select(i => new { i.Code, i.Field, i.Message }), Manifest = JsonSerializer.Deserialize<JsonElement>(JsonFormatter.Default.Format(value.Manifest ?? new Google.Protobuf.WellKnownTypes.Struct())), value.RequestedPermissions, value.Signed, value.SignerKeyId, value.DataPaths };
    private static IResult Accepted(Operation op) => Results.Accepted($"/api/v1/operations/{op.Id}", CoreClient.ToOperation(op));
    public static void MapExtensions(this RouteGroupBuilder api)
    {
        api.MapGet("/extensions/folders", (CoreClient core, HttpContext context) => Run(async () =>
        {
            var folders = new List<object>(); var cursor = "";
            do
            {
                var page = await core.Control.ListResourcesAsync(new ListResourcesRequest { Kind = ResourceKind.ManagedFolder, Page = new PageRequest { PageSize = 100, PageToken = cursor } }, Headers(context, false));
                foreach (var row in page.Resources)
                {
                    var spec = JsonSerializer.Deserialize<JsonElement>(JsonFormatter.Default.Format(row.Spec));
                    var mode = spec.GetProperty("storageMode").GetString();
                    folders.Add(new { id = row.Ref.Id, name = spec.GetProperty("name").GetString(), storageMode = mode, @protected = mode == "HybridBackup" && (!spec.TryGetProperty("advanced", out var advanced) || !advanced.TryGetProperty("backupEnabled", out var enabled) || enabled.ValueKind != JsonValueKind.False) });
                }
                cursor = page.NextPageToken;
            } while (cursor.Length > 0);
            return Results.Ok(new { folders });
        })).WithMetadata(new AccessScope("R"));
        api.MapGet("/subsystems", (CoreClient core, HttpContext context) => Run(async () =>
        {
            var response = await core.Extensions.ListSubsystemsAsync(new Empty(), Headers(context, false));
            return Results.Ok(new { response.Subsystems, response.DefaultProviders });
        })).WithMetadata(new AccessScope("R"));
        api.MapPut("/capabilities/{capability}/default", (string capability, DefaultProvider body, CoreClient core, HttpContext context, AuditWriter audit) => Run(async () =>
        {
            await core.Extensions.SetDefaultProviderAsync(new SetDefaultProviderRequest { Capability = capability, SubsystemId = body.SubsystemId }, Headers(context, true)); await Audit(audit, context, "capability.default", capability); return Results.NoContent();
        })).WithMetadata(new AccessScope("F"));
        api.MapPost("/extensions/inspect", (HttpContext context, IConfiguration config, CoreClient core) => Run(async () =>
        {
            var id = await Stage(context, config); var response = await core.Extensions.InspectPackageAsync(new InspectPackageRequest { PackagePath = PathFor(config, id) }, Headers(context, false)); if (!response.Valid) { File.Delete(PathFor(config, id)); }
            return Results.Ok(Inspection(response, id));
        })).WithMetadata(new AccessScope("F"));
        api.MapPost("/extensions", (InstallExtension body, HttpContext context, IConfiguration config, CoreClient core, AuditWriter audit) => Run(async () =>
        {
            var request = new InstallExtensionRequest { PackagePath = PathFor(config, body.StagedPackageId), ApprovedPermissions = { body.ApprovedPermissions ?? [] }, AcknowledgedUnsigned = body.AcknowledgedUnsigned, AcknowledgedUnprotectedData = body.AcknowledgedUnprotectedData }; foreach (var (name, id) in body.DataBindings ?? []) { request.DataBindings[name] = id; }
            var op = await core.Extensions.InstallExtensionAsync(request, Headers(context, true)); File.Delete(request.PackagePath); await Audit(audit, context, "extension.install", op.Id); return Accepted(op);
        })).WithMetadata(new AccessScope("F"));
        api.MapPost("/extensions/{id}/upgrade", (string id, UpgradeExtension body, HttpContext context, IConfiguration config, CoreClient core, AuditWriter audit) => Run(async () =>
        {
            var request = new UpgradeExtensionRequest { SubsystemId = id, PackagePath = PathFor(config, body.StagedPackageId), ApprovedPermissions = { body.ApprovedPermissions ?? [] }, AcknowledgedUnsigned = body.AcknowledgedUnsigned }; var op = await core.Extensions.UpgradeExtensionAsync(request, Headers(context, true)); File.Delete(request.PackagePath); await Audit(audit, context, "extension.upgrade", id); return Accepted(op);
        })).WithMetadata(new AccessScope("F"));
        api.MapPost("/extensions/{id}/restart", (string id, HttpContext context, CoreClient core, AuditWriter audit) => Run(async () =>
        {
            var op = await core.Control.StartOperationAsync(new StartOperationRequest { Kind = "RestartExtension", Params = Google.Protobuf.WellKnownTypes.Struct.Parser.ParseJson(JsonSerializer.Serialize(new { subsystemId = id })) }, Headers(context, true));
            await Audit(audit, context, "extension.restart", id); return Accepted(op);
        })).WithMetadata(new AccessScope("F"));
        api.MapDelete("/extensions/{id}", (string id, HttpContext context, CoreClient core, AuditWriter audit) => Run(async () =>
        {
            var op = await core.Extensions.RemoveExtensionAsync(new SubsystemRef { SubsystemId = id }, Headers(context, true)); await Audit(audit, context, "extension.remove", id); return Accepted(op);
        })).WithMetadata(new AccessScope("F"));
        api.MapPost("/extensions/{id}/package", (string id, HttpContext context, IConfiguration config, CoreClient core, AuditWriter audit) => Run(async () =>
        {
            var staged = await Stage(context, config); var inspected = await core.Extensions.InspectPackageAsync(new InspectPackageRequest { PackagePath = PathFor(config, staged) }, Headers(context, false));
            if (!inspected.Valid || inspected.Manifest.Fields["id"].StringValue != id) { File.Delete(PathFor(config, staged)); return Results.Problem(statusCode: 422, detail: "extension.recorded_package_required"); }
            var op = await core.Extensions.InstallExtensionAsync(new InstallExtensionRequest { PackagePath = PathFor(config, staged) }, Headers(context, true)); File.Delete(PathFor(config, staged)); await Audit(audit, context, "extension.package_supplied", id); return Accepted(op);
        })).WithMetadata(new AccessScope("F"));
    }
}
public sealed record DefaultProvider(string SubsystemId);
public sealed record InstallExtension(string StagedPackageId, string[] ApprovedPermissions, bool AcknowledgedUnsigned, Dictionary<string, string> DataBindings, bool AcknowledgedUnprotectedData);
public sealed record UpgradeExtension(string StagedPackageId, string[] ApprovedPermissions, bool AcknowledgedUnsigned);
