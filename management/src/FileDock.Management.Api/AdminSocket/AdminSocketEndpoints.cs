using System.Net.Sockets;
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

            return await next(invocation);
        });
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
