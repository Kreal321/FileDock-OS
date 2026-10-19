using FileDock.Management.Core;

namespace FileDock.Management.Api.Endpoints;

public static class SystemEndpoints
{
    private static readonly string[] _missingChannelNotice = ["No notification channel is configured yet."];

    public static RouteGroupBuilder MapSystem(this RouteGroupBuilder api)
    {
        api.MapGet("/system/status", GetStatus).WithMetadata(new Auth.AccessScope("R"));
        return api;
    }

    private static async Task<IResult> GetStatus(HttpContext context, ICoreGateway core, CoreHealthMonitor monitor)
    {
        if (context.User.Identity?.IsAuthenticated != true)
        {
            return Results.Unauthorized();
        }

        var status = monitor.LastStatus ?? new SystemStatusDto("", "", "", [], false, true);

        if (monitor.Current.CanMutate)
        {
            using var timeout = CancellationTokenSource.CreateLinkedTokenSource(context.RequestAborted);
            timeout.CancelAfter(TimeSpan.FromSeconds(2));
            try { status = await core.GetSystemStatusAsync(timeout.Token); }
            catch (Exception error) when (error is Grpc.Core.RpcException or OperationCanceledException) { monitor.ReportUnavailable(); }
        }
        var components = status.Components.ToList();
        if (components.All(c => c.Id != "management"))
        {
            components.Insert(Math.Min(1, components.Count), new ComponentHealthDto("management", "Management Service", "Healthy", "serving"));
        }

        if (!monitor.Current.CanMutate)
        {
            components.RemoveAll(c => c.Id == "core");
            components.Insert(0, new ComponentHealthDto("core", "FileDock Core", monitor.Current.State, monitor.Current.Detail));
        }
        var notices = new List<string>();
        if (status.NotificationChannelMissing) { notices.AddRange(_missingChannelNotice); }
        if (status.BasicClaimSafetyWarning) { notices.Add("location.basic_claim_safety: One or more owned locations use basic claim safety. Conditional ownership claims are not enforced by their connector."); }
        return Results.Ok(new
        {
            interfaceVersion = status.InterfaceVersion,
            releaseVersion = status.ReleaseVersion,
            applianceName = status.ApplianceName,
            components,
            stateProtectionWarning = status.StateProtectionWarning,
            notificationChannelMissing = status.NotificationChannelMissing,
            basicClaimSafetyWarning = status.BasicClaimSafetyWarning,
            notices,
        });
    }
}
