using System.Security.Claims;
using System.Text.Encodings.Web;
using FileDock.Management.Domain.Sessions;
using FileDock.Management.Domain.Stores;
using Microsoft.AspNetCore.Authentication;
using Microsoft.Extensions.Options;

namespace FileDock.Management.Api.Auth;

public sealed class SessionAuthHandler(
    IOptionsMonitor<AuthenticationSchemeOptions> options,
    ILoggerFactory logger,
    UrlEncoder encoder,
    SessionService sessions,
    IAdminStore admins) : AuthenticationHandler<AuthenticationSchemeOptions>(options, logger, encoder)
{
    public const string SchemeName = "fd-session";
    public const string CookieName = "fd_http_session";
    public const string CsrfCookie = "fd_http_csrf";
    public const string CsrfHeader = "X-CSRF-Token";

    protected override async Task<AuthenticateResult> HandleAuthenticateAsync()
    {
        if (!Request.Cookies.TryGetValue(CookieName, out var token) || string.IsNullOrEmpty(token))
        {
            return AuthenticateResult.NoResult();
        }

        var session = await sessions.AuthenticateAsync(token);
        if (session is null)
        {
            return AuthenticateResult.Fail("Session expired.");
        }

        var admin = await admins.FindByIdAsync(session.AdministratorId);
        if (admin is null || !admin.Enabled)
        {
            return AuthenticateResult.Fail("Administrator is not available.");
        }

        Context.Items["fd.session"] = session;
        Context.Items["fd.admin"] = admin;
        var identity = new ClaimsIdentity(
            [
                new Claim(ClaimTypes.NameIdentifier, admin.Id.ToString()),
                new Claim(ClaimTypes.Name, admin.Username),
            ],
            SchemeName);
        return AuthenticateResult.Success(new AuthenticationTicket(new ClaimsPrincipal(identity), SchemeName));
    }

    public static bool CsrfValid(HttpContext context)
    {
        if (HttpMethods.IsGet(context.Request.Method) || HttpMethods.IsHead(context.Request.Method))
        {
            return true;
        }

        if (context.Items["fd.session"] is not FileDock.Management.Domain.Session session)
        {
            return true;
        }

        var header = context.Request.Headers[CsrfHeader].ToString();
        return !string.IsNullOrEmpty(header) && CryptographicEquals(header, session.CsrfToken);
    }

    private static bool CryptographicEquals(string left, string right)
    {
        var a = System.Text.Encoding.UTF8.GetBytes(left);
        var b = System.Text.Encoding.UTF8.GetBytes(right);
        return a.Length == b.Length && System.Security.Cryptography.CryptographicOperations.FixedTimeEquals(a, b);
    }
}
