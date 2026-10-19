using System.Net.Sockets;
using Grpc.Core;
using FileDock.Management.Core;
using FileDock.Management.Api.Endpoints;
using FileDock.Management.Api.Network;
using FileDock.Management.Domain.Admins;
using FileDock.Management.Domain.Audit;
using FileDock.Management.Domain.Sessions;
using FileDock.Management.Domain.Setup;
using FileDock.Management.Domain.Stores;
using FileDock.Management.Domain.Tokens;
using Microsoft.AspNetCore.Connections.Features;

namespace FileDock.Management.Api.AdminSocket;

public static class AdminSocketEndpoints
{
    public static void Map(WebApplication app)
    {
        var admin = app.MapGroup("/admin").WithDisplayName("admin-socket");
        admin.AddEndpointFilter(async (invocation, next) =>
        {
            if (!IsLocalAdmin(invocation.HttpContext))
            {
                return Results.StatusCode(403);
            }

            try
            {
                return await next(invocation);
            }
            catch (RpcException error) when (error.StatusCode is not StatusCode.Internal and not StatusCode.Unknown and not StatusCode.DataLoss)
            {
                var (status, detail) = CoreClient.ToProblem(error);
                return Results.Problem(
                    statusCode: status == 412 ? 409 : status,
                    title: detail == "restore.target_not_fresh" ? "Restore requires a fresh appliance" : "Console request could not finish",
                    detail: detail);
            }
        });
        admin.MapPost("/connectors", (CoreClient core) => ObservabilityEndpoints.Run(async () => Results.Ok(ResourceEndpoints.Json(await core.Connectors.ListConnectorsAsync(new Filedock.Core.V1.Empty(), CoreClient.Metadata("Console", "connectors.list", false))))));
        admin.MapPost("/connector-routes", (CoreClient core) => ObservabilityEndpoints.Run(async () => Results.Ok(ConnectorEndpoints.Routes(await core.Connectors.GetRoutesAsync(new Filedock.Core.V1.Empty(), CoreClient.Metadata("Console", "routes.show", false))))));
        admin.MapPost("/connections/test", async (ConnectionTestInput input, CoreClient core, AuditWriter audit) => await ObservabilityEndpoints.Run(async () => {
            string? id = null; var cursor = "";
            do { var page = await core.Control.ListResourcesAsync(new Filedock.Core.V1.ListResourcesRequest { Kind = Filedock.Core.V1.ResourceKind.CloudConnection, Page = new Filedock.Core.V1.PageRequest { PageSize = 500, PageToken = cursor } }, CoreClient.Metadata("Console", "connections.test", false)); id ??= page.Resources.FirstOrDefault(r => r.Spec.Fields.TryGetValue("name", out var value) && value.StringValue == input.Name)?.Ref.Id; cursor = page.NextPageToken; } while (cursor.Length > 0 && id is null);
            if (id is null) { return Results.Problem(statusCode: 404, detail: "connection.not_found"); }
            var result = await core.Connectors.TestConnectionAsync(new Filedock.Core.V1.ConnectionRef { ConnectionId = id }, CoreClient.Metadata("Console", "connections.test", true));
            await audit.WriteAsync("Console", "root", "root", "connection.tested", id, "Succeeded"); return Results.Ok(ResourceEndpoints.Json(result));
        }));
        admin.MapRecoveryConsole();
        admin.MapPost("/diagnostics", async (FileDock.Management.Core.CoreClient core, AuditWriter audit, HttpContext context) => await ObservabilityEndpoints.Run(async () => { var data = await ObservabilityEndpoints.Bundle(core, FileDock.Management.Core.CoreClient.Metadata("Console", "diagnostics", true), context.RequestAborted); await audit.WriteAsync("Console", "root", "root", "diagnostics.exported", "appliance", "Succeeded"); return Results.File(data, "application/zstd", "filedock-diagnostics.tar.zst"); }));
        admin.MapPost("/reset-password", async (ConsolePassword body, AdminService admins, IAdminStore store, SessionService sessions, AuditWriter audit) =>
            await AdminEndpoints.Run(async () =>
            {
                var account = await store.FindByUsernameAsync((body.Username ?? "").Trim().ToLowerInvariant()) ?? throw new KeyNotFoundException("Administrator not found.");
                await admins.ChangePasswordAsync(account.Id, body.Password ?? "");
                await sessions.SignOutAllAsync(account.Id);
                await audit.WriteAsync("Console", "root", "root", "admin.password-reset", account.Id.ToString(), "Succeeded");
                return Results.Ok(new { ok = true });
            }));
        admin.MapPost("/tokens/revoke-all", async (ApiTokenService tokens, AuditWriter audit) =>
        {
            await tokens.RevokeAllAsync();
            await audit.WriteAsync("Console", "root", "root", "token.revoke-all", "api-tokens", "Succeeded");
            return Results.Ok(new { ok = true });
        });
        admin.MapPost("/network/reset", async (HttpContext context, NetworkService network, AuditWriter audit) =>
        {
            var addresses = await network.SaveAsync(NetworkService.DefaultAddresses, null, true);
            await audit.WriteAsync("Console", "root", "root", "network.reset", "management-listener", "Succeeded");
            context.Response.OnCompleted(() => { network.Apply(addresses); return Task.CompletedTask; });
            return Results.Ok(new { ok = true });
        });
        admin.MapGet("/ping", (HttpContext context) =>
        {
            if (!IsLocalAdmin(context))
            {
                return Results.StatusCode(StatusCodes.Status403Forbidden);
            }

            return Results.Ok(new { ok = true });
        });
        admin.MapPost("/setup-code", async (HttpContext context, SetupCodeService setup) =>
        {
            if (!IsLocalAdmin(context))
            {
                return Results.StatusCode(StatusCodes.Status403Forbidden);
            }

            try
            {
                var code = await setup.IssueAsync();
                return Results.Ok(new { code });
            }
            catch (InvalidOperationException ex)
            {
                return Results.Json(new { detail = ex.Message }, statusCode: 409);
            }
        });
    }

    public static bool IsLocalAdmin(HttpContext context)
    {
        return context.Features.Get<IConnectionSocketFeature>()?.Socket.LocalEndPoint is UnixDomainSocketEndPoint;
    }
}

public sealed record ConsolePassword(string? Username, string? Password);

public sealed record ConnectionTestInput(string Name);
