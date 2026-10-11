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
    }

    public static bool IsLocalAdmin(HttpContext context)
    {
        return context.Connection.RemoteIpAddress is null
            || context.Connection.LocalIpAddress is null;
    }
}
