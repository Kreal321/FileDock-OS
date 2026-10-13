using System.Security.Claims;
using System.Text.Encodings.Web;
using FileDock.Management.Domain.Tokens;
using Microsoft.AspNetCore.Authentication;
using Microsoft.Extensions.Options;

namespace FileDock.Management.Api.Auth;

public sealed class TokenAuthHandler(
    IOptionsMonitor<AuthenticationSchemeOptions> options, ILoggerFactory logger, UrlEncoder encoder,
    ApiTokenService tokens) : AuthenticationHandler<AuthenticationSchemeOptions>(options, logger, encoder)
{
    public const string SchemeName = "fd-token";
    protected override async Task<AuthenticateResult> HandleAuthenticateAsync()
    {
        var header = Request.Headers.Authorization.ToString();
        if (!header.StartsWith("Bearer ", StringComparison.OrdinalIgnoreCase))
        {
            return AuthenticateResult.NoResult();
        }

        var token = await tokens.AuthenticateAsync(header[7..].Trim());
        if (token is null)
        {
            return AuthenticateResult.Fail("Invalid, revoked, or expired token.");
        }

        Context.Items["fd.token"] = token;
        var identity = new ClaimsIdentity([
            new Claim(ClaimTypes.NameIdentifier, token.Id.ToString()),
            new Claim(ClaimTypes.Name, token.Name), new Claim("fd.scope", token.Scope),
        ], SchemeName);
        return AuthenticateResult.Success(new AuthenticationTicket(new ClaimsPrincipal(identity), SchemeName));
    }
}
