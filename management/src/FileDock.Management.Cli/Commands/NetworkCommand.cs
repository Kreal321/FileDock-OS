namespace FileDock.Management.Cli.Commands;

public static class NetworkCommand
{
    public static Task<int> RunAsync(string socketPath, bool json) => AdminSocketClient.PostAsync(socketPath, "/admin/network/reset", null, json);
}
