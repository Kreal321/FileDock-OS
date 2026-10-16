namespace FileDock.Management.Cli.Commands;

internal static class SystemPoolCommand
{
    internal static Task<int> Show(string socket, bool json) => AdminSocketClient.PostAsync(socket, "/admin/system-pool", null, json);
    internal static Task<int> Limits(string socket, uint? percent, ulong? reserve, bool acknowledged, bool json) => percent is null && reserve is null ? Show(socket, json) : AdminSocketClient.PostAsync(socket, "/admin/system-pool/limits", new { maxUsagePercent = percent, reservedFreeGb = reserve, acknowledgedInterruption = acknowledged }, json);
}
