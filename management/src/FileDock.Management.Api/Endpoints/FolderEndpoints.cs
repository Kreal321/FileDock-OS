using System.Text.Json;
using Filedock.Core.V1;
using FileDock.Management.Api.Auth;
using FileDock.Management.Api.Infrastructure;
using FileDock.Management.Core;
using FileDock.Management.Domain.Audit;
using Google.Protobuf;
using Google.Protobuf.WellKnownTypes;
using Grpc.Core;
namespace FileDock.Management.Api.Endpoints;

public static class FolderEndpoints
{
    private static readonly JsonSerializerOptions Json = new(JsonSerializerDefaults.Web);
    private static readonly (string Id, string[] Capabilities)[] Modes = [("LocalOnly", ["storage.local"]), ("CloudMount", ["cloud.mount", "cache.media", "cache.generic"]), ("HybridBackup", ["backup.snapshot", "backup.restore", "backup.retention"]), ("TieredStorage", ["tier.storage"])];
    private static JsonElement Element(IMessage message) => JsonSerializer.Deserialize<JsonElement>(JsonFormatter.Default.Format(message));
    private static object View(Resource row)
    {
        var spec = Element(row.Spec); var status = row.Status is null ? (JsonElement?)null : Element(row.Status);
        var mode=spec.GetProperty("storageMode").GetString();
        var current=status is JsonElement observed && observed.TryGetProperty("observedGeneration",out var g) && g.TryGetUInt64(out var observedGeneration) && observedGeneration==row.Generation;
        if(!current){status=JsonSerializer.SerializeToElement(new{state="Pending",detail="Configuration is awaiting reconciliation."});}
        var protection=current && status is JsonElement observedStatus && observedStatus.TryGetProperty("protectionState",out var state)?state.GetString():"AtRisk";
        var exposure=current && status is JsonElement observedExposure && observedExposure.TryGetProperty("exposure",out var text)?text.GetString():"Protection observations are pending. Changes without a durable copy may be lost.";
        if(mode is "LocalOnly" or "TieredStorage"){protection="Unprotected";exposure=mode=="LocalOnly"?"All data is kept only on this appliance and may be lost if its pool fails.":"The local-only hot tier may be lost until verified cloud copies exist.";}
        if(mode=="HybridBackup" && spec.TryGetProperty("dataImportance",out var level) && level.GetString()=="Rebuildable" && spec.TryGetProperty("advanced",out var policy) && (!policy.TryGetProperty("backupEnabled",out var enabled)||enabled.ValueKind!=JsonValueKind.True)){protection="NotRequired";exposure="Rebuildable data is not backed up by choice and must be reconnected or rebuilt after loss.";}
        return new { Id = row.Ref.Id, row.Generation, Identity = new { Name = spec.GetProperty("name").GetString(), FileDockPath = spec.GetProperty("filedockPath").GetString() }, StorageMode = spec.GetProperty("storageMode").GetString(), Reference = spec.GetProperty("reference"), DataImportance = spec.TryGetProperty("dataImportance", out var importance) ? importance.GetString() : null, Access = spec.TryGetProperty("access", out var access) ? access : JsonSerializer.SerializeToElement(Array.Empty<string>()), Advanced = spec.TryGetProperty("advanced", out var advanced) ? advanced : JsonSerializer.SerializeToElement(new { }), Status = status, ProtectionState = protection, Exposure = exposure };
    }
    private static object Report(ValidationReport report) => new { report.Valid, Issues = report.Issues.Select(i => new { Severity = i.Severity.ToString(), i.Code, i.Field, i.Message, Conflicting = i.Conflicting?.Id }) };
    private static ResourceRef Ref(string id) => new() { Kind = ResourceKind.ManagedFolder, Id = id };
    private static ApplyChangeRequest Change(FolderInput input, string id, ulong generation)
    {
        if (input.Identity is null || string.IsNullOrWhiteSpace(input.Identity.Name)) { throw new ArgumentException("Folder identity and name are required."); }
        var path = input.Identity.FileDockPath;
        if (string.IsNullOrEmpty(path)) { path = "/storage/" + string.Concat(input.Identity.Name.ToLowerInvariant().Select(c => char.IsAsciiLetterOrDigit(c) || c is '-' or '_' ? c : '-')).Trim('-'); }
        var spec = new { name = input.Identity.Name, filedockPath = path, input.StorageMode, Reference = input.Reference.ValueKind == JsonValueKind.Undefined ? JsonSerializer.SerializeToElement(new { }) : input.Reference, input.DataImportance, Access = input.Access ?? [], Advanced = input.Advanced.ValueKind == JsonValueKind.Undefined ? JsonSerializer.SerializeToElement(new { }) : input.Advanced };
        var request = new ApplyChangeRequest { Changes = { new ResourceChange { Upsert = new ResourceChange.Types.Upsert { Ref = Ref(id), ExpectedGeneration = generation, Spec = Struct.Parser.ParseJson(JsonSerializer.Serialize(spec, Json)) } } } };
        if (input.ConfirmImportanceReduction) { request.Confirmations.Add(new ChangeConfirmation { Resource = Ref(id), Kind = "lowerImportance", Value = "confirmed" }); }
        if (input.ConfirmModeChange) { request.Confirmations.Add(new ChangeConfirmation { Resource = Ref(id), Kind = "leaveHybridBackup", Value = "confirmed" }); }
        return request;
    }
    private static async Task<List<Resource>> All(CoreClient core, ResourceKind kind, HttpContext c)
    {
        var rows = new List<Resource>(); string cursor = "";
        do { var page = await core.Control.ListResourcesAsync(new ListResourcesRequest { Kind = kind, Page = new PageRequest { PageSize = 500, PageToken = cursor } }, ObservabilityEndpoints.Headers(c, false)); rows.AddRange(page.Resources); cursor = page.NextPageToken; } while (cursor.Length > 0);
        return rows;
    }
    private static async Task<IResult> Save(FolderInput input, string id, HttpContext c, CoreClient core, AuditWriter audit)
    {
        return await ObservabilityEndpoints.Run(async () =>
        {
            ulong generation = 0; Resource? previous = null;
            if (!string.IsNullOrEmpty(id)) { var match = Concurrency.IfMatch(c.Request); if (match is null || match < 1) { return Results.Problem(statusCode: 428, detail: "Supply the current generation using If-Match."); } generation = (ulong)match; previous = await core.Control.GetResourceAsync(Ref(id), ObservabilityEndpoints.Headers(c, false)); }
            var request = Change(input, id, generation);
            if(string.IsNullOrEmpty(id)){id=Guid.CreateVersion7().ToString();request.Changes[0].Upsert.Ref.Id=id;}
            var exports=await All(core,ResourceKind.Export,c);
            // Access selection creates declarative exports in the same atomic batch. Phase 11 applies their access rules.
            foreach (var protocol in input.Access ?? [])
            {
                if (protocol is not ("Smb" or "Nfs" or "WebDav")) { continue; }
                // Creation needs an explicit folder UUID so both resources refer to the same identity.
                if (string.IsNullOrEmpty(id)) { id = Guid.CreateVersion7().ToString(); request.Changes[0].Upsert.Ref.Id = id; }
                if (!exports.Any(e => Element(e.Spec).GetProperty("folderId").GetString() == id && Element(e.Spec).GetProperty("protocol").GetString() == protocol)) { request.Changes.Add(new ResourceChange { Upsert = new ResourceChange.Types.Upsert { Ref = new ResourceRef { Kind = ResourceKind.Export }, Spec = Struct.Parser.ParseJson(JsonSerializer.Serialize(new { folderId = id, protocol, rules = Array.Empty<object>() })) } }); }
            }
            foreach(var export in exports.Where(e=>Element(e.Spec).GetProperty("folderId").GetString()==id && !(input.Access??[]).Contains(Element(e.Spec).GetProperty("protocol").GetString()))){request.Changes.Add(new ResourceChange{Delete=new ResourceChange.Types.Delete{Ref=export.Ref,ExpectedGeneration=export.Generation}});}
            var report=await core.Control.ValidateChangeAsync(request,ObservabilityEndpoints.Headers(c,false));
            if(!report.Valid){return Results.Json(Report(report),statusCode:422);}
            var result = await core.Control.ApplyChangeAsync(request, ObservabilityEndpoints.Headers(c, true));
            var folder = result.Committed.Single(r => r.Ref.Kind == ResourceKind.ManagedFolder);
            await ObservabilityEndpoints.Audit(c, audit, string.IsNullOrEmpty(c.Request.RouteValues["id"]?.ToString()) ? "folder.created" : "folder.updated", folder.Ref.Id);
            if (previous is not null && (Element(previous.Spec).GetRawText() != Element(folder.Spec).GetRawText())) { await ObservabilityEndpoints.Audit(c, audit, "folder.policy.changed", folder.Ref.Id, new { PreviousImportance = Element(previous.Spec).TryGetProperty("dataImportance", out var oldImportance) ? oldImportance.GetString() : null, NewImportance = input.DataImportance, PreviousMode = Element(previous.Spec).GetProperty("storageMode").GetString(), NewMode = input.StorageMode }); }
            if (input.ConfirmImportanceReduction) { await ObservabilityEndpoints.Audit(c, audit, "folder.importance-reduced.confirmed", folder.Ref.Id); }
            if (input.ConfirmModeChange) { await ObservabilityEndpoints.Audit(c, audit, "folder.importance-removed.confirmed", folder.Ref.Id); }
            Concurrency.SetGeneration(c.Response, (long)folder.Generation);
            return Results.Json(View(folder), statusCode: generation == 0 ? 201 : 200);
        });
    }
    public static void MapFolders(this RouteGroupBuilder api)
    {
        api.MapGet("/folders/options", (CoreClient core, HttpContext c) => ObservabilityEndpoints.Run(async () =>
        {
            var subsystems = await core.Extensions.ListSubsystemsAsync(new Filedock.Core.V1.Empty(), ObservabilityEndpoints.Headers(c, false)); var caps = subsystems.Subsystems.Where(s => s.State == "Running").SelectMany(s => s.Capabilities).Append("storage.local").ToHashSet();
            var modes = Modes;
            var pools = await All(core, ResourceKind.Pool, c); var connections = await All(core, ResourceKind.CloudConnection, c);
            return Results.Ok(new { Modes = modes.Select(m => new { Id = m.Item1, Enabled = m.Item2.All(caps.Contains), MissingCapabilities = m.Item2.Where(v => !caps.Contains(v)) }), Pools = pools.Select(p => new { Id = p.Ref.Id, Name = Element(p.Spec).GetProperty("name").GetString(), Kind = Element(p.Spec).GetProperty("kind").GetString(), Status = p.Status is null ? (JsonElement?)null : Element(p.Status) }), Connections = connections.Select(r => new { Id = r.Ref.Id, Name = Element(r.Spec).GetProperty("name").GetString() }), Access = new[] { ("Smb", "protocol.smb"), ("Nfs", "protocol.nfs"), ("WebDav", "protocol.webdav"), ("Docker", "runtime.container") }.Select(a => new { Id = a.Item1, Enabled = caps.Contains(a.Item2), MissingCapability = caps.Contains(a.Item2) ? null : a.Item2 }) });
        })).WithMetadata(new AccessScope("R"));
        api.MapGet("/folders", (CoreClient core, HttpContext c) => ObservabilityEndpoints.Run(async () =>
        { var p = PageQuery.From(c.Request); var page = await core.Control.ListResourcesAsync(new ListResourcesRequest { Kind = ResourceKind.ManagedFolder, Page = new PageRequest { PageSize = (uint)Math.Clamp(p.PageSize, 1, 500), PageToken = p.PageToken ?? "" } }, ObservabilityEndpoints.Headers(c, false)); return Results.Ok(new { Items = page.Resources.Select(View), NextPageToken = string.IsNullOrEmpty(page.NextPageToken) ? null : page.NextPageToken }); })).WithMetadata(new AccessScope("R"));
        api.MapGet("/folders/{id}", (string id, CoreClient core, HttpContext c) => ObservabilityEndpoints.Run(async () => { var row = await core.Control.GetResourceAsync(Ref(id), ObservabilityEndpoints.Headers(c, false)); Concurrency.SetGeneration(c.Response, (long)row.Generation); return Results.Ok(View(row)); })).WithMetadata(new AccessScope("R"));
        api.MapPost("/folders/validate", (FolderInput input, HttpContext c, CoreClient core) => ObservabilityEndpoints.Run(async () => { var id = c.Request.Query["id"].ToString(); ulong generation = 0; if (id.Length > 0) { generation = (await core.Control.GetResourceAsync(Ref(id), ObservabilityEndpoints.Headers(c, false))).Generation; } return Results.Ok(Report(await core.Control.ValidateChangeAsync(Change(input, id, generation), ObservabilityEndpoints.Headers(c, false)))); })).WithMetadata(new AccessScope("F"));
        api.MapPost("/folders", (FolderInput input, HttpContext c, CoreClient core, AuditWriter audit) => Save(input, "", c, core, audit)).WithMetadata(new AccessScope("F"));
        api.MapPatch("/folders/{id}", (string id, FolderInput input, HttpContext c, CoreClient core, AuditWriter audit) => Save(input, id, c, core, audit)).WithMetadata(new AccessScope("F"));
        api.MapDelete("/folders/{id}", (string id, HttpContext c, CoreClient core, AuditWriter audit) => ObservabilityEndpoints.Run(async () => { var match = Concurrency.IfMatch(c.Request); if (match is null || match < 1) { return Results.Problem(statusCode: 428, detail: "Supply the current generation using If-Match."); } var r = new ApplyChangeRequest { Changes = { new ResourceChange { Delete = new ResourceChange.Types.Delete { Ref = Ref(id), ExpectedGeneration = (ulong)match } } } }; r.Confirmations.Add(new ChangeConfirmation { Resource = Ref(id), Kind = "deleteCritical", Value = c.Request.Query["typedName"].ToString() }); await core.Control.ApplyChangeAsync(r, ObservabilityEndpoints.Headers(c, true)); await ObservabilityEndpoints.Audit(c, audit, "folder.deleted", id); return Results.NoContent(); })).WithMetadata(new AccessScope("F"));
    }
}
public sealed record FolderIdentity(string Name, string? FileDockPath);
public sealed record FolderInput(FolderIdentity? Identity, string StorageMode, JsonElement Reference, string? DataImportance, string[]? Access, JsonElement Advanced, bool ConfirmImportanceReduction = false, bool ConfirmModeChange = false);
