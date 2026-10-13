using FileDock.Management.Api.Auth;
using FileDock.Management.Domain;
using FileDock.Management.Domain.Admins;
using FileDock.Management.Domain.Audit;
using FileDock.Management.Domain.Sessions;
using FileDock.Management.Domain.Stores;

namespace FileDock.Management.Api.Endpoints;

public static class AdminEndpoints
{
    public static void MapAdmins(this RouteGroupBuilder api)
    {
        var group = api.MapGroup("/admins").WithMetadata(new AccessScope("S"));
        group.MapGet("", async (IAdminStore store) => Results.Ok((await store.ListAsync()).Select(View)));
        group.MapPost("", async (CreateAdmin body, HttpContext context, AdminService admins, AuditWriter audit) =>
            await Run(async () =>
            {
                Match(body.Password, body.PasswordConfirmation);
                var created = await admins.CreateAsync(body.Username ?? "", body.Password ?? "");
                await WriteAudit(context, audit, "admin.create", created.Id);
                return Results.Created($"/api/v1/admins/{created.Id}", View(created));
            }));
        group.MapPatch("/{id:guid}", async (Guid id, UpdateAdmin body, HttpContext context, AdminService admins, SessionService sessions, AuditWriter audit) =>
            await Run(async () =>
            {
                await admins.UpdateAsync(id, body.Username, body.Enabled);
                if (body.Enabled == false)
                {
                    await sessions.SignOutAllAsync(id);
                }

                await WriteAudit(context, audit, "admin.update", id);
                return Results.NoContent();
            }));
        group.MapDelete("/{id:guid}", async (Guid id, HttpContext context, AdminService admins, SessionService sessions, AuditWriter audit) =>
            await Run(async () =>
            {
                await admins.DeleteAsync(id);
                await sessions.SignOutAllAsync(id);
                await WriteAudit(context, audit, "admin.delete", id);
                return Results.NoContent();
            }));
        group.MapPost("/{id:guid}/password", async (Guid id, ChangePassword body, HttpContext context, AdminService admins, SessionService sessions, AuditWriter audit) =>
            await Run(async () =>
            {
                var actor = (Administrator)context.Items["fd.admin"]!;
                if (actor.Id != id)
                {
                    return Results.StatusCode(403);
                }

                Match(body.Password, body.PasswordConfirmation);
                await admins.AuthenticateAsync(actor.Username, body.CurrentPassword ?? "");
                await admins.ChangePasswordAsync(id, body.Password ?? "");
                await sessions.SignOutAllAsync(id);
                AuthEndpoints.ClearCookies(context);
                await WriteAudit(context, audit, "admin.password-change", id);
                return Results.NoContent();
            }));
    }
    public static object View(Administrator a) => new { a.Id, a.Username, a.Enabled, a.CreatedAt, a.LastSignInAt, a.LockedUntil };
    public static void Match(string? password, string? confirmation)
    {
        if (password != confirmation)
        {
            throw new ArgumentException("Passwords do not match.");
        }
    }
    public static Task WriteAudit(HttpContext context, AuditWriter audit, string action, Guid id)
    {
        var actor = (Administrator)context.Items["fd.admin"]!;
        return audit.WriteAsync("Administrator", actor.Id.ToString(), actor.Username, action, id.ToString(), "Succeeded");
    }
    public static async Task<IResult> Run(Func<Task<IResult>> action)
    {
        try { return await action(); }
        catch (KeyNotFoundException ex) { return Problem(404, ex.Message); }
        catch (LastAdminException ex) { return Problem(409, ex.Message); }
        catch (AccountLockedException ex) { return Problem(429, ex.Message); }
        catch (UnauthorizedAccessException ex) { return Problem(401, ex.Message); }
        catch (ArgumentException ex) { return Problem(400, ex.Message); }
        catch (InvalidOperationException ex) { return Problem(409, ex.Message); }
    }
    public static IResult Problem(int status, string detail) => Results.Json(new { type = "about:blank", title = "Request refused", status, detail }, contentType: "application/problem+json", statusCode: status);
}
public sealed record CreateAdmin(string? Username, string? Password, string? PasswordConfirmation);
public sealed record UpdateAdmin(string? Username, bool? Enabled);
public sealed record ChangePassword(string? CurrentPassword, string? Password, string? PasswordConfirmation);
