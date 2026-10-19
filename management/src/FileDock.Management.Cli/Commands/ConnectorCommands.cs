namespace FileDock.Management.Cli.Commands;
internal static class ConnectorCommands
{
    public static Task<int> List(string socket, bool json) => AdminSocketClient.PostAsync(socket, "/admin/connectors", null, json, showResult: true);
    public static Task<int> Routes(string socket, bool json) => AdminSocketClient.PostAsync(socket, "/admin/connector-routes", null, json, showResult: true);
    public static Task<int> Test(string socket, string name, bool json) => AdminSocketClient.PostAsync(socket, "/admin/connections/test", new { name }, json, showResult: true);
}
