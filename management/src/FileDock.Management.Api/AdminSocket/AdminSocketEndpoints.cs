using FileDock.Management.Domain.Setup;

namespace FileDock.Management.Api.AdminSocket;

public static class AdminSocketEndpoints
{
    public static void Map(WebApplication app)
    {
        var admin = app.MapGroup("/admin").WithDisplayName("admin-socket");
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
        return context.Connection.RemoteIpAddress is null
            || context.Connection.LocalIpAddress is null;
    }
}
