using System.CommandLine;
using System.Text.Json;

var json = new Option<bool>("--json", "-j") { Description = "Print machine-readable JSON" };
var root = new RootCommand("filedockctl — local administration (root only)") { json };
root.SetAction((parse) =>
{
    if (parse.GetValue(json))
    {
        Console.WriteLine(JsonSerializer.Serialize(new { ok = true, tool = "filedockctl" }));
    }
    else
    {
        Console.WriteLine("filedockctl is ready. Commands are added as stories land.");
    }

    return 0;
});

return await root.Parse(args).InvokeAsync();
