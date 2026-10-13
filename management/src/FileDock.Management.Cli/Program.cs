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

return await root.Parse(args).InvokeAsync();
