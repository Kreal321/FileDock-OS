using System.CommandLine;
using FileDock.Management.Cli.Commands;

var json = new Option<bool>("--json", "-j") { Description = "Print machine-readable JSON", Recursive = true };
var socket = new Option<string>("--socket")
{
    Description = "Management admin socket",
    Recursive = true,
    DefaultValueFactory = _ => "/run/filedock/management-admin.sock",
};
var root = new RootCommand("filedockctl — local administration (root only)") { json, socket };

var setupCode = new Command("setup-code", "Print a fresh setup code if setup is not complete");
setupCode.SetAction(async parse =>
{
    var socketPath = parse.GetValue(socket) ?? "/run/filedock/management-admin.sock";
    return await SetupCodeCommand.RunAsync(socketPath, parse.GetValue(json));
});
root.Add(setupCode);

var username = new Argument<string>("username");
var resetPassword = new Command("reset-password", "Set an administrator password and unlock the account") { username };
resetPassword.SetAction(async parse => await ResetPasswordCommand.RunAsync(parse.GetValue(socket)!, parse.GetValue(username)!, parse.GetValue(json)));
root.Add(resetPassword);
var tokens = new Command("tokens", "Manage API tokens from the console");
var revokeAll = new Command("revoke-all", "Immediately revoke every API token");
revokeAll.SetAction(async parse => await TokensCommand.RunAsync(parse.GetValue(socket)!, parse.GetValue(json)));
tokens.Add(revokeAll);
root.Add(tokens);
var network = new Command("network", "Recover management listener access");
var resetNetwork = new Command("reset", "Restore HTTP management access on all interfaces");
resetNetwork.SetAction(async parse => await NetworkCommand.RunAsync(parse.GetValue(socket)!, parse.GetValue(json)));
network.Add(resetNetwork);
root.Add(network);

var diagnosticsOutput = new Argument<string>("output");
var diagnostics = new Command("diagnostics", "Save a redacted diagnostic tar.zst to a new file") { diagnosticsOutput };
diagnostics.SetAction(async parse => await DiagnosticsCommand.RunAsync(parse.GetValue(socket)!, parse.GetValue(diagnosticsOutput)!)); root.Add(diagnostics);
var state = new Command("state", "Configuration recovery and snapshots");
var stateStatus = new Command("status", "Inspect configuration recovery coverage");
stateStatus.SetAction(async parse => await AdminSocketClient.PostAsync(parse.GetValue(socket)!, "/admin/state-protection", null, true)); state.Add(stateStatus);
var backupOutput = new Argument<string>("output");
var backup = new Command("export", "Save a configuration .fdbackup to a new file") { backupOutput };
backup.SetAction(async parse => await StateCommands.Export(parse.GetValue(socket)!, parse.GetValue(backupOutput)!)); state.Add(backup); root.Add(state);
var recovery = new Command("recovery-key", "Manage the configuration recovery key");
var recoveryOutput = new Argument<string>("output");
var regenerate = new Command("regenerate", "Rotate the recovery key and save it to a new private text file") { recoveryOutput };
regenerate.SetAction(async parse => await StateCommands.Regenerate(parse.GetValue(socket)!, parse.GetValue(recoveryOutput)!)); recovery.Add(regenerate); root.Add(recovery);
var detect = new Command("detect-state", "Find surviving local configuration copies");
detect.SetAction(async parse => await AdminSocketClient.PostAsync(parse.GetValue(socket)!, "/admin/detect-state", null, true)); root.Add(detect);
var restore = new Command("restore-local", "Restore the newest verified local snapshot using only the recovery key");
restore.SetAction(async parse => await StateCommands.Restore(parse.GetValue(socket)!, parse.GetValue(json))); root.Add(restore);
var systemPool = new Command("system-pool", "Inspect the System Pool or change its reserve");
var showPool = new Command("show", "Show capacity, reserve, and availability");
showPool.SetAction(async parse => await SystemPoolCommand.Show(parse.GetValue(socket)!,parse.GetValue(json)));
systemPool.Add(showPool);
var percent = new Option<uint?>("--max-usage");
var reserve = new Option<ulong?>("--reserve");
var interruption = new Option<bool>("--acknowledge-interruption");
var limits = new Command("limits", "Change limits; shrinking can interrupt folder access") {percent,reserve,interruption};
limits.SetAction(async parse=>await SystemPoolCommand.Limits(parse.GetValue(socket)!,parse.GetValue(percent),parse.GetValue(reserve),parse.GetValue(interruption),parse.GetValue(json)));
systemPool.Add(limits);root.Add(systemPool);
var connectors = new Command("connectors", "Inspect cloud connectors");
var listConnectors = new Command("list", "List connector families, health and routing");
listConnectors.SetAction(async p => await ConnectorCommands.List(p.GetValue(socket)!, p.GetValue(json))); connectors.Add(listConnectors); root.Add(connectors);
var routes = new Command("routes", "Inspect connector routing");
var showRoutes = new Command("show", "Show family defaults and connection overrides");
showRoutes.SetAction(async p => await ConnectorCommands.Routes(p.GetValue(socket)!, p.GetValue(json))); routes.Add(showRoutes); root.Add(routes);
var connections = new Command("connections", "Inspect cloud connections");
var connectionName = new Argument<string>("name");
var testConnection = new Command("test", "Test a connection by its name") { connectionName };
testConnection.SetAction(async p => await ConnectorCommands.Test(p.GetValue(socket)!, p.GetValue(connectionName)!, p.GetValue(json))); connections.Add(testConnection); root.Add(connections);
root.SetAction(parse =>
{
    if (parse.GetValue(json))
    {
        Console.WriteLine("""{"ok":true,"tool":"filedockctl"}""");
    }
    else
    {
        Console.WriteLine("filedockctl is ready. Commands are added as stories land.");
    }

    return 0;
});

if (!OperatingSystem.IsLinux() || NativeIdentity.GetEffectiveUserId() != 0)
{
    Console.Error.WriteLine("filedockctl requires root on the appliance's Linux console.");
    return 1;
}
try { return await root.Parse(args).InvokeAsync(); }
catch (Exception ex) when (ex is HttpRequestException or System.Net.Sockets.SocketException or IOException)
{
    Console.Error.WriteLine($"Cannot reach the Management Service: {ex.Message}");
    return 1;
}
internal static partial class NativeIdentity
{
    [System.Runtime.InteropServices.LibraryImport("libc", EntryPoint = "geteuid")]
    internal static partial uint GetEffectiveUserId();
}
