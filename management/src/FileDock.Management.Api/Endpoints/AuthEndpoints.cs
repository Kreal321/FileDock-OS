using FileDock.Management.Api.Auth;
using FileDock.Management.Domain;
using FileDock.Management.Domain.Admins;
using FileDock.Management.Domain.Audit;
using FileDock.Management.Domain.Sessions;

namespace FileDock.Management.Api.Endpoints;

public static class AuthEndpoints
{
    public static RouteGroupBuilder MapAuth(this RouteGroupBuilder api)
    {
        api.MapPost("/auth/sign-in", SignIn);
        api.MapPost("/auth/sign-out", SignOut);
        api.MapPost("/auth/sign-out-all", SignOutAll);
        return api;
    }

    private static async Task<IResult> SignIn(
        SignInRequest body,
        HttpContext context,
        AdminService admins,
        SessionService sessions,
        AuditWriter audit)
    {
        var ip = context.Connection.RemoteIpAddress?.ToString() ?? "unknown";
        try
        {
            var admin = await admins.AuthenticateAsync(body.Username ?? "", body.Password ?? "");
            var issued = await sessions.CreateAsync(admin, ip, context.Request.Headers.UserAgent);
            WriteCookies(context, issued);
            await audit.WriteAsync("Administrator", admin.Id.ToString(), admin.Username, "auth.sign-in", "session", "Succeeded");
            return Results.Ok(new { username = admin.Username });
        }
        catch (AccountLockedException ex)
        {
            await audit.WriteAsync("Administrator", body.Username ?? "", body.Username ?? "", "auth.sign-in", "session", "Refused", new { reason = "locked" });
            return Results.Json(new { type = "about:blank", title = "Locked", status = 429, detail = ex.Message, until = ex.Until }, statusCode: 429);
        }
        catch (UnauthorizedAccessException)
        {
            await audit.WriteAsync("Administrator", body.Username ?? "", body.Username ?? "", "auth.sign-in", "session", "Failed");
            return Results.Json(new { type = "about:blank", title = "Unauthorized", status = 401, detail = "Invalid username or password." }, statusCode: 401);
        }
    }

    private static async Task<IResult> SignOut(HttpContext context, SessionService sessions, AuditWriter audit)
    {
        if (context.Items["fd.session"] is Session session && context.Items["fd.admin"] is Administrator admin)
        {
            await sessions.SignOutAsync(session);
            await audit.WriteAsync("Administrator", admin.Id.ToString(), admin.Username, "auth.sign-out", "session", "Succeeded");
        }

        ClearCookies(context);
        return Results.NoContent();
    }

    private static async Task<IResult> SignOutAll(HttpContext context, SessionService sessions, AuditWriter audit)
    {
        if (context.Items["fd.admin"] is not Administrator admin)
        {
            return Results.Unauthorized();
        }

        await sessions.SignOutAllAsync(admin.Id);
        await audit.WriteAsync("Administrator", admin.Id.ToString(), admin.Username, "auth.sign-out-all", "session", "Succeeded");
        ClearCookies(context);
        return Results.NoContent();
    }

    public static void WriteCookies(HttpContext context, IssuedSession issued)
    {
        context.Response.Cookies.Append(SessionAuthHandler.CookieName, issued.Token, new CookieOptions
        {
            HttpOnly = true,
            Secure = false,
            SameSite = SameSiteMode.Strict,
            Path = "/",
        });
        context.Response.Cookies.Append(SessionAuthHandler.CsrfCookie, issued.Csrf, new CookieOptions
        {
            HttpOnly = false,
            Secure = false,
            SameSite = SameSiteMode.Strict,
            Path = "/",
        });
    }

    public static void ClearCookies(HttpContext context)
    {
        context.Response.Cookies.Delete(SessionAuthHandler.CookieName);
        context.Response.Cookies.Delete(SessionAuthHandler.CsrfCookie);
    }
}

public sealed record SignInRequest(string? Username, string? Password);
