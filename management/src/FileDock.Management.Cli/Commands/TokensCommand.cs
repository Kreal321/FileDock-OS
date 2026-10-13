namespace FileDock.Management.Cli.Commands;

public static class TokensCommand
{
    public static Task<int> RunAsync(string socketPath, bool json) => AdminSocketClient.PostAsync(socketPath, "/admin/tokens/revoke-all", null, json);
}
