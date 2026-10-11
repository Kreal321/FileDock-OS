using FileDock.Management.Core;

namespace FileDock.Management.Api.Endpoints;

public static class SystemEndpoints
{
    private static readonly string[] MissingChannelNotice = ["No notification channel is configured yet."];

    public static RouteGroupBuilder MapSystem(this RouteGroupBuilder api)
    {
        api.MapGet("/system/status", GetStatus);
        return api;
    }

    private static async Task<IResult> GetStatus(HttpContext context, ICoreGateway core)
    {
        if (context.User.Identity?.IsAuthenticated != true)
        {
            return Results.Unauthorized();
        }

        var status = await core.GetSystemStatusAsync();
        var components = status.Components.ToList();
        if (components.All(c => c.Id != "management"))
        {
            components.Insert(1, new ComponentHealthDto("management", "Management Service", "Healthy", "serving"));
        }

        return Results.Ok(new
        {
            interfaceVersion = status.InterfaceVersion,
            releaseVersion = status.ReleaseVersion,
            applianceName = status.ApplianceName,
            components,
            stateProtectionWarning = status.StateProtectionWarning,
            notificationChannelMissing = status.NotificationChannelMissing,
            notices = status.NotificationChannelMissing ? MissingChannelNotice : Array.Empty<string>(),
        });
    }
}
