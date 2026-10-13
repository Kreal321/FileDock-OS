using FileDock.Management.Api.Auth;
using FileDock.Management.Core;
using FileDock.Management.Domain.Admins;
using FileDock.Management.Domain.Audit;
using FileDock.Management.Domain.Security;
using FileDock.Management.Domain.Sessions;
using FileDock.Management.Domain.Setup;
using FileDock.Management.Domain.Stores;
using Grpc.Core;

namespace FileDock.Management.Api.Endpoints;

public static class SetupEndpoints
{
    public static RouteGroupBuilder MapSetup(this RouteGroupBuilder api)
    {
        api.MapPost("/setup/verify-code", Verify).WithMetadata(new Auth.AccessScope("P"));
        api.MapPost("/setup/complete", Complete).WithMetadata(new Auth.AccessScope("P"));
        api.MapPost("/setup/recovery-key/confirm", Confirm).WithMetadata(new Auth.AccessScope("S"));
        return api;
    }

    private static async Task<IResult> Verify(SetupCodeRequest body, HttpContext context, SetupCodeService setup)
    {
        try
        {
            await setup.VerifyAsync(body.Code ?? "", ClientIp(context));
            return Results.Ok(new { ok = true });
        }
        catch (SetupThrottleException ex)
        {
            return Problem(429, "Throttled", ex.Message);
        }
        catch (ArgumentException ex)
        {
            return Problem(400, "Invalid setup code", ex.Message);
        }
        catch (InvalidOperationException ex)
        {
            return Problem(409, "Setup already complete", ex.Message);
        }
    }

    private static readonly SemaphoreSlim _setupGate = new(1, 1);

    private static async Task<IResult> Complete(
        SetupCompleteRequest body, HttpContext context, SetupCodeService setup,
        AdminService admins, SessionService sessions, ICoreGateway core, AuditWriter audit, IAdminStore adminStore)
    {
        await _setupGate.WaitAsync(context.RequestAborted);
        try
        {
            return await CompleteInternal(body, context, setup, admins, sessions, core, audit, adminStore);
        }
        finally
        {
            _setupGate.Release();
        }
    }

    private static async Task<IResult> CompleteInternal(
        SetupCompleteRequest body,
        HttpContext context,
        SetupCodeService setup,
        AdminService admins,
        SessionService sessions,
        ICoreGateway core,
        AuditWriter audit,
        IAdminStore adminStore)
    {
        try
        {
            await setup.VerifyAsync(body.Code ?? "", ClientIp(context));
        }
        catch (SetupThrottleException ex)
        {
            return Problem(429, "Throttled", ex.Message);
        }
        catch (ArgumentException ex)
        {
            return Problem(400, "Invalid setup code", ex.Message);
        }

        catch (InvalidOperationException ex)
        {
            return Problem(409, "Setup already complete", ex.Message);
        }

        try
        {
            PasswordHasher.EnsurePolicy(body.AdminPassword ?? "");
            if (!string.Equals(body.AdminPassword, body.AdminPasswordConfirmation, StringComparison.Ordinal))
            {
                return Problem(400, "Invalid setup", "Passwords do not match.");
            }
            var username = (body.AdminUsername ?? "").Trim().ToLowerInvariant();
            if (username.Length is < 1 or > 64 || string.IsNullOrWhiteSpace(body.ApplianceName))
            {
                return Problem(400, "Invalid setup", "Supply an appliance name and a username of 1–64 characters.");
            }

            // Resume a setup whose response was lost, without creating another administrator.
            var existing = await adminStore.ListAsync();
            FileDock.Management.Domain.Administrator? admin = null;
            if (existing.Count > 0)
            {
                admin = await admins.AuthenticateAsync(username, body.AdminPassword ?? "");
            }
            var requestId = Guid.NewGuid().ToString("N");
            await core.ApplyApplianceNameAsync(body.ApplianceName, "setup", requestId);
            var key = await core.GenerateRecoveryKeyAsync($"admin:{username}", requestId);
            admin ??= await admins.CreateAsync(username, body.AdminPassword ?? "");
            var issued = await sessions.CreateAsync(admin, ClientIp(context), context.Request.Headers.UserAgent);
            AuthEndpoints.WriteCookies(context, issued);
            await audit.WriteAsync("Administrator", admin.Id.ToString(), admin.Username, "setup.administrator", "appliance", "Succeeded");
            return Results.Ok(new
            {
                username = admin.Username,
                applianceName = body.ApplianceName,
                keyId = key.KeyId,
                recoveryKey = key.RecoveryKey,
            });
        }
        catch (RpcException)
        {
            return Problem(503, "Core unavailable", "Setup could not finish. Retry when the Core service is available.");
        }
        catch (Exception ex) when (ex is ArgumentException or InvalidOperationException or UnauthorizedAccessException)
        {
            return Problem(400, "Setup failed", ex.Message);
        }
    }

    private static async Task<IResult> Confirm(
        HttpContext context,
        ConfirmRecoveryRequest body,
        ICoreGateway core,
        SetupCodeService setup,
        AuditWriter audit)
    {
        if (context.Items["fd.admin"] is not FileDock.Management.Domain.Administrator admin)
        {
            return Results.Unauthorized();
        }

        if (!SessionAuthHandler.CsrfValid(context))
        {
            return Results.Json(new { type = "about:blank", title = "CSRF", status = 403, detail = "CSRF token missing or invalid." }, statusCode: 403);
        }

        try
        {
            await core.ConfirmRecoveryKeyAsync(
                body.KeyId ?? "",
                $"admin:{admin.Username}",
                Guid.NewGuid().ToString("N"));
        }
        catch (Exception ex) when (ex is ArgumentException or InvalidOperationException or RpcException)
        {
            return Problem(400, "Confirmation failed", ex.Message);
        }

        await setup.MarkCompleteAsync();
        await audit.WriteAsync("Administrator", admin.Id.ToString(), admin.Username, "setup.recovery-key.confirm", "recovery-key", "Succeeded");
        return Results.Ok(new { complete = true });
    }

    private static string ClientIp(HttpContext context) =>
        context.Connection.RemoteIpAddress?.ToString() ?? "local";

    private static IResult Problem(int status, string title, string detail) =>
        Results.Json(new { type = "about:blank", title, status, detail }, statusCode: status);
}

public sealed record SetupCodeRequest(string? Code);
public sealed record SetupCompleteRequest(string? Code, string? ApplianceName, string? AdminUsername, string? AdminPassword, string? AdminPasswordConfirmation);
public sealed record ConfirmRecoveryRequest(string? KeyId);
